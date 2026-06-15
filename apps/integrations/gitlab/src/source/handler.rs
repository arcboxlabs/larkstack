//! Axum handler for `POST /webhooks/gitlab/webhook` — authenticates the request,
//! filters by project whitelist, dispatches on `object_kind`, and posts Lark cards.

use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use lark_kit::Live;
use lark_kit::card::{LarkCard, LarkMessage};
use tracing::{info, warn};

use super::payload::{
    EventProbe, IssueEvent, MergeRequestEvent, NoteEvent, PipelineEvent, PushEvent,
};
use super::verify;
use crate::cards;
use crate::config::{AppState, GitLabConfig};

async fn post_group(state: &AppState, card: &LarkMessage) {
    lark_kit::send_lark_card(&state.http, &state.lark.webhook_url, card).await;
}

async fn dm(state: &AppState, email: &str, card: &LarkCard) {
    if let Some(bot) = &state.bot
        && let Err(e) = bot.send_dm(email, card).await
    {
        warn!("failed to DM {email}: {e}");
    }
}

/// Handles incoming GitLab webhook requests.
pub async fn webhook_handler(
    Live(state): Live<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let gitlab = &state.gitlab;

    if !verify::authenticate(&headers, &body, gitlab) {
        warn!("GitLab webhook authentication failed");
        return StatusCode::UNAUTHORIZED;
    }

    // Discriminator + project (for the whitelist gate) in one minimal parse.
    let probe = match serde_json::from_slice::<EventProbe>(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!("invalid GitLab payload: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let project = probe
        .project
        .map(|p| p.path_with_namespace)
        .unwrap_or_default();

    if !gitlab.project_whitelist.is_empty()
        && (project.is_empty() || !gitlab.project_whitelist.contains(&project))
    {
        info!("ignoring event from non-whitelisted project: {project:?}");
        return StatusCode::OK;
    }

    match probe.object_kind.as_str() {
        "merge_request" => handle_merge_request(&state, gitlab, &body).await,
        "issue" => handle_issue(&state, gitlab, &body).await,
        "pipeline" => handle_pipeline(&state, &body).await,
        "note" => handle_note(&state, &body).await,
        "push" => handle_push(&state, &body).await,
        other => {
            info!("ignoring GitLab event kind: {other}");
            StatusCode::OK
        }
    }
}

async fn handle_merge_request(
    state: &Arc<AppState>,
    gitlab: &GitLabConfig,
    body: &[u8],
) -> StatusCode {
    let ev: MergeRequestEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse merge_request event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let repo = &ev.project.path_with_namespace;
    let a = &ev.object_attributes;
    let author = &ev.user.name;
    let action = a.action.as_deref().unwrap_or("");

    match action {
        "merge" => {
            info!("GitLab MR merged: {repo}!{}", a.iid);
            post_group(
                state,
                &cards::mr_merged(repo, a.iid, &a.title, author, &a.url),
            )
            .await;
        }
        "open" | "reopen" => {
            info!("GitLab MR opened: {repo}!{}", a.iid);
            post_group(
                state,
                &cards::mr_opened(
                    repo,
                    a.iid,
                    &a.title,
                    author,
                    &a.source_branch,
                    &a.target_branch,
                    &a.url,
                ),
            )
            .await;
            // DM each mapped reviewer (or assignee, if no reviewers are set).
            let targets = if ev.reviewers.is_empty() {
                &ev.assignees
            } else {
                &ev.reviewers
            };
            for t in targets {
                if let Some(email) = gitlab.user_map.get(&t.username) {
                    dm(
                        state,
                        email,
                        &cards::mr_review_dm(repo, a.iid, &a.title, author, &a.url),
                    )
                    .await;
                }
            }
        }
        other => info!(
            "ignoring merge_request action '{other}' for {repo}!{}",
            a.iid
        ),
    }
    StatusCode::OK
}

async fn handle_issue(state: &Arc<AppState>, gitlab: &GitLabConfig, body: &[u8]) -> StatusCode {
    let ev: IssueEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse issue event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let Some(label) = newly_added_alert_label(&ev, &gitlab.alert_labels) else {
        info!("ignoring issue event with no newly-added alert label");
        return StatusCode::OK;
    };
    let repo = &ev.project.path_with_namespace;
    let a = &ev.object_attributes;
    info!("GitLab issue labeled alert: {repo}#{} label={label}", a.iid);
    post_group(
        state,
        &cards::issue_labeled(repo, a.iid, &a.title, label, &ev.user.name, &a.url),
    )
    .await;
    StatusCode::OK
}

/// The first label added in this event (present in `changes.current` but not
/// `changes.previous`) whose lowercased title is in `alert_labels`.
fn newly_added_alert_label<'a>(ev: &'a IssueEvent, alert_labels: &[String]) -> Option<&'a str> {
    let labels = ev.changes.as_ref()?.labels.as_ref()?;
    let previous: HashSet<&str> = labels.previous.iter().map(|l| l.title.as_str()).collect();
    labels
        .current
        .iter()
        .filter(|l| !previous.contains(l.title.as_str()))
        .find(|l| alert_labels.contains(&l.title.to_lowercase()))
        .map(|l| l.title.as_str())
}

async fn handle_pipeline(state: &Arc<AppState>, body: &[u8]) -> StatusCode {
    let ev: PipelineEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse pipeline event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let a = &ev.object_attributes;
    if a.status != "failed" {
        info!("ignoring pipeline with status '{}'", a.status);
        return StatusCode::OK;
    }
    let repo = &ev.project.path_with_namespace;
    let commit_title = ev.commit.as_ref().map(|c| c.title.as_str());
    info!("GitLab pipeline failed: {repo} ref={}", a.ref_name);
    post_group(
        state,
        &cards::pipeline_failed(repo, &a.ref_name, &ev.user.name, commit_title, &a.url),
    )
    .await;
    StatusCode::OK
}

async fn handle_note(state: &Arc<AppState>, body: &[u8]) -> StatusCode {
    let ev: NoteEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse note event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let a = &ev.object_attributes;
    let noteable = match a.noteable_type.as_str() {
        "MergeRequest" => ev
            .merge_request
            .as_ref()
            .map(|p| format!("MR: {}", p.title)),
        "Issue" => ev.issue.as_ref().map(|p| format!("Issue: {}", p.title)),
        "Snippet" => ev.snippet.as_ref().map(|p| format!("Snippet: {}", p.title)),
        "Commit" => Some("a commit".to_string()),
        _ => None,
    }
    .unwrap_or_else(|| a.noteable_type.clone());
    let repo = &ev.project.path_with_namespace;
    let snippet = lark_kit::truncate(a.note.trim(), 200);
    info!("GitLab note on {noteable}: {repo}");
    post_group(
        state,
        &cards::note(repo, &noteable, &ev.user.name, &snippet, &a.url),
    )
    .await;
    StatusCode::OK
}

async fn handle_push(state: &Arc<AppState>, body: &[u8]) -> StatusCode {
    let ev: PushEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse push event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    if ev.total_commits_count == 0 {
        info!("ignoring push with no commits (branch create/delete)");
        return StatusCode::OK;
    }
    let repo = &ev.project.path_with_namespace;
    let branch = ev
        .ref_name
        .strip_prefix("refs/heads/")
        .unwrap_or(&ev.ref_name);
    info!("GitLab push: {repo} branch={branch}");
    post_group(
        state,
        &cards::push(
            repo,
            branch,
            &ev.user_username,
            ev.total_commits_count,
            &ev.commits,
        ),
    )
    .await;
    StatusCode::OK
}
