//! Axum handler for `POST /webhooks/gitlab/webhook` — authenticates the request, dispatches
//! on `object_kind`, and delivers Lark cards to the console-configured routing destinations.

use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use lark_kit::Live;
use lark_kit::routing::{Config, Destination, deliver, deliver_all};
use tracing::{info, warn};

use super::payload::{
    IssueEvent, KindProbe, MergeRequestEvent, NoteEvent, PipelineEvent, PushEvent,
};
use super::verify;
use crate::cards;
use crate::config::AppState;

/// Handles incoming GitLab webhook requests.
pub async fn webhook_handler(
    Live(state): Live<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if !verify::authenticate(&headers, &body, &state.gitlab) {
        warn!("GitLab webhook authentication failed");
        return StatusCode::UNAUTHORIZED;
    }

    let probe = match serde_json::from_slice::<KindProbe>(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!("invalid GitLab payload: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    // Live routing config — loaded per webhook so console edits apply without a restart.
    let cfg = Config::load(&state.store, "gitlab").await;

    match probe.object_kind.as_str() {
        "merge_request" => handle_merge_request(&state, &cfg, &body).await,
        "issue" => handle_issue(&state, &cfg, &body).await,
        "pipeline" => handle_pipeline(&state, &cfg, &body).await,
        "note" => handle_note(&state, &cfg, &body).await,
        "push" => handle_push(&state, &cfg, &body).await,
        other => {
            info!("ignoring GitLab event kind: {other}");
            StatusCode::OK
        }
    }
}

async fn handle_merge_request(state: &Arc<AppState>, cfg: &Config, body: &[u8]) -> StatusCode {
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
    let dests = cfg.destinations_for(repo, "merge_request");

    match action {
        "merge" => {
            info!("GitLab MR merged: {repo}!{}", a.iid);
            let card = cards::mr_merged(repo, a.iid, &a.title, author, &a.url);
            deliver_all(state.bot.as_ref(), &dests, &card).await;
        }
        "open" | "reopen" => {
            info!("GitLab MR opened: {repo}!{}", a.iid);
            let card = cards::mr_opened(
                repo,
                a.iid,
                &a.title,
                author,
                &a.source_branch,
                &a.target_branch,
                &a.url,
            );
            deliver_all(state.bot.as_ref(), &dests, &card).await;
            // Reviewer/assignee DMs (independent of routing): DM each mapped user directly.
            let targets = if ev.reviewers.is_empty() {
                &ev.assignees
            } else {
                &ev.reviewers
            };
            let dm_card = cards::mr_review_dm(repo, a.iid, &a.title, author, &a.url);
            for t in targets {
                if let Some(email) = cfg.lark_email(&t.username) {
                    deliver(state.bot.as_ref(), &Destination::dm(email), &dm_card).await;
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

async fn handle_issue(state: &Arc<AppState>, cfg: &Config, body: &[u8]) -> StatusCode {
    let ev: IssueEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to parse issue event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };
    let Some(label) = newly_added_alert_label(&ev, cfg) else {
        info!("ignoring issue event with no newly-added alert label");
        return StatusCode::OK;
    };
    let repo = &ev.project.path_with_namespace;
    let a = &ev.object_attributes;
    info!("GitLab issue labeled alert: {repo}#{} label={label}", a.iid);
    let card = cards::issue_labeled(repo, a.iid, &a.title, label, &ev.user.name, &a.url);
    deliver_all(
        state.bot.as_ref(),
        &cfg.destinations_for(repo, "issue"),
        &card,
    )
    .await;
    StatusCode::OK
}

/// The first label added in this event (present in `changes.current` but not
/// `changes.previous`) that is configured as an alert label.
fn newly_added_alert_label<'a>(ev: &'a IssueEvent, cfg: &Config) -> Option<&'a str> {
    let labels = ev.changes.as_ref()?.labels.as_ref()?;
    let previous: std::collections::HashSet<&str> =
        labels.previous.iter().map(|l| l.title.as_str()).collect();
    labels
        .current
        .iter()
        .filter(|l| !previous.contains(l.title.as_str()))
        .find(|l| cfg.is_alert_label(&l.title))
        .map(|l| l.title.as_str())
}

async fn handle_pipeline(state: &Arc<AppState>, cfg: &Config, body: &[u8]) -> StatusCode {
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
    let card = cards::pipeline_failed(repo, &a.ref_name, &ev.user.name, commit_title, &a.url);
    deliver_all(
        state.bot.as_ref(),
        &cfg.destinations_for(repo, "pipeline"),
        &card,
    )
    .await;
    StatusCode::OK
}

async fn handle_note(state: &Arc<AppState>, cfg: &Config, body: &[u8]) -> StatusCode {
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
    let card = cards::note(repo, &noteable, &ev.user.name, &snippet, &a.url);
    deliver_all(
        state.bot.as_ref(),
        &cfg.destinations_for(repo, "note"),
        &card,
    )
    .await;
    StatusCode::OK
}

async fn handle_push(state: &Arc<AppState>, cfg: &Config, body: &[u8]) -> StatusCode {
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
    let card = cards::push(
        repo,
        branch,
        &ev.user_username,
        ev.total_commits_count,
        &ev.commits,
    );
    deliver_all(
        state.bot.as_ref(),
        &cfg.destinations_for(repo, "push"),
        &card,
    )
    .await;
    StatusCode::OK
}
