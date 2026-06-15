//! Axum handler for `POST /github/webhook` — verifies the signature, filters by
//! repo whitelist, parses with octocrab's native `WebhookEvent` models, and
//! posts a Lark card.

use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use lark_kit::Live;
use lark_kit::card::{LarkCard, LarkMessage};
use octocrab::models::webhook_events::{
    WebhookEvent, WebhookEventPayload,
    payload::{
        DependabotAlertWebhookEventAction, IssuesWebhookEventAction, PullRequestWebhookEventAction,
        SecretScanningAlertWebhookEventAction, WorkflowRunWebhookEventAction,
    },
};
use tracing::{info, warn};

use super::utils::verify_github_signature;
use crate::{
    cards,
    config::{AppState, GitHubConfig},
};

/// Pre-parse: extracts `repository.name` (whitelist) and `full_name` (display)
/// without deserializing the full payload.
#[derive(serde::Deserialize)]
struct RepoProbe {
    repository: RepoName,
}

#[derive(serde::Deserialize)]
struct RepoName {
    name: String,
    full_name: Option<String>,
}

/// Inner data for `workflow_run` events — octocrab keeps it as a
/// `serde_json::Value`, so we deserialize the fields we need manually.
#[derive(serde::Deserialize)]
struct WorkflowRunData {
    conclusion: Option<String>,
    name: String,
    head_branch: String,
    actor: WorkflowRunActor,
    html_url: String,
}

#[derive(serde::Deserialize)]
struct WorkflowRunActor {
    login: String,
}

#[derive(serde::Deserialize)]
struct SecretScanningAlertData {
    secret_type_display_name: Option<String>,
    secret_type: String,
    html_url: String,
}

#[derive(serde::Deserialize)]
struct DependabotAlertData {
    severity: String,
    dependency: Option<DependabotDependency>,
    security_advisory: Option<DependabotAdvisory>,
    html_url: String,
}

#[derive(serde::Deserialize)]
struct DependabotDependency {
    package: Option<DependabotPackage>,
}

#[derive(serde::Deserialize)]
struct DependabotPackage {
    name: String,
}

#[derive(serde::Deserialize)]
struct DependabotAdvisory {
    summary: String,
}

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

/// Handles incoming GitHub webhook requests.
pub async fn webhook_handler(
    Live(state): Live<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let github = &state.github;

    let Some(signature) = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    else {
        warn!("missing x-hub-signature-256 header");
        return StatusCode::UNAUTHORIZED;
    };
    if !verify_github_signature(&github.webhook_secret, &body, signature) {
        warn!("invalid GitHub webhook signature");
        return StatusCode::UNAUTHORIZED;
    }

    // Extract repo name (whitelist) and full_name (display) in one pass.
    let (repo, repo_name) = match serde_json::from_slice::<RepoProbe>(&body) {
        Ok(probe) => {
            let full = probe
                .repository
                .full_name
                .clone()
                .unwrap_or_else(|| probe.repository.name.clone());
            (full, probe.repository.name)
        }
        Err(_) => {
            warn!("could not extract repository name from payload");
            (String::new(), String::new())
        }
    };

    if !github.repo_whitelist.is_empty()
        && (repo_name.is_empty() || !github.repo_whitelist.contains(&repo_name))
    {
        info!("ignoring event from non-whitelisted repo: {repo_name:?}");
        return StatusCode::OK;
    }

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let webhook = match WebhookEvent::try_from_header_and_body(event_type, &body) {
        Ok(ev) => ev,
        Err(e) => {
            warn!("failed to parse GitHub webhook event: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    match webhook.specific {
        WebhookEventPayload::PullRequest(payload) => {
            handle_pull_request(&state, github, &repo, *payload).await
        }
        WebhookEventPayload::Issues(payload) => {
            handle_issues(&state, github, &repo, *payload).await
        }
        WebhookEventPayload::WorkflowRun(payload) => {
            handle_workflow_run(&state, &repo, *payload).await
        }
        WebhookEventPayload::SecretScanningAlert(payload) => {
            handle_secret_scanning(&state, &repo, *payload).await
        }
        WebhookEventPayload::DependabotAlert(payload) => {
            handle_dependabot(&state, &repo, *payload).await
        }
        _ => {
            info!("ignoring GitHub event type: {event_type}");
            StatusCode::OK
        }
    }
}

async fn handle_pull_request(
    state: &Arc<AppState>,
    github: &GitHubConfig,
    repo: &str,
    payload: octocrab::models::webhook_events::payload::PullRequestWebhookEventPayload,
) -> StatusCode {
    let pr = &payload.pull_request;
    let number = payload.number;
    let title = pr.title.clone().unwrap_or_default();
    let author = pr
        .user
        .as_ref()
        .map(|u| u.login.clone())
        .unwrap_or_default();
    let html_url = pr
        .html_url
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();

    match payload.action {
        PullRequestWebhookEventAction::Opened => {
            info!("GitHub PR opened: {repo}#{number}");
            post_group(
                state,
                &cards::pr_opened(
                    repo,
                    number,
                    &title,
                    &author,
                    &pr.head.ref_field,
                    &pr.base.ref_field,
                    pr.additions.unwrap_or(0),
                    pr.deletions.unwrap_or(0),
                    &html_url,
                ),
            )
            .await;
            StatusCode::OK
        }
        PullRequestWebhookEventAction::ReviewRequested => {
            let Some(reviewer) = payload.requested_reviewer.as_ref().map(|u| u.login.clone())
            else {
                info!("review_requested without requested_reviewer, ignoring");
                return StatusCode::OK;
            };
            info!("GitHub review requested: {repo}#{number} reviewer={reviewer}");
            let reviewer_lark_id = github.user_map.get(&reviewer).cloned();
            post_group(
                state,
                &cards::pr_review_requested(
                    repo,
                    number,
                    &title,
                    &author,
                    &reviewer,
                    reviewer_lark_id.as_deref(),
                    &html_url,
                ),
            )
            .await;
            if let Some(email) = reviewer_lark_id {
                dm(
                    state,
                    &email,
                    &cards::pr_review_dm(repo, number, &title, &author, &html_url),
                )
                .await;
            }
            StatusCode::OK
        }
        PullRequestWebhookEventAction::Closed if pr.merged_at.is_some() => {
            let merged_by = pr
                .merged_by
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_else(|| author.clone());
            info!("GitHub PR merged: {repo}#{number} by {merged_by}");
            post_group(
                state,
                &cards::pr_merged(repo, number, &title, &author, &merged_by, &html_url),
            )
            .await;
            StatusCode::OK
        }
        _ => {
            info!("ignoring pull_request action for {repo}#{number}");
            StatusCode::OK
        }
    }
}

async fn handle_issues(
    state: &Arc<AppState>,
    github: &GitHubConfig,
    repo: &str,
    payload: octocrab::models::webhook_events::payload::IssuesWebhookEventPayload,
) -> StatusCode {
    if payload.action != IssuesWebhookEventAction::Labeled {
        info!("ignoring issues action");
        return StatusCode::OK;
    }
    let Some(label) = payload.label.as_ref().map(|l| l.name.clone()) else {
        return StatusCode::OK;
    };
    if !github.alert_labels.contains(&label.to_lowercase()) {
        info!("ignoring non-alert label: {label}");
        return StatusCode::OK;
    }
    let issue = &payload.issue;
    info!(
        "GitHub issue labeled alert: {repo}#{} label={label}",
        issue.number
    );
    post_group(
        state,
        &cards::issue_labeled(
            repo,
            issue.number,
            &issue.title,
            &label,
            &issue.user.login,
            issue.html_url.as_ref(),
        ),
    )
    .await;
    StatusCode::OK
}

async fn handle_workflow_run(
    state: &Arc<AppState>,
    repo: &str,
    payload: octocrab::models::webhook_events::payload::WorkflowRunWebhookEventPayload,
) -> StatusCode {
    if payload.action != WorkflowRunWebhookEventAction::Completed {
        info!("ignoring workflow_run action");
        return StatusCode::OK;
    }
    let run: WorkflowRunData = match serde_json::from_value(payload.workflow_run) {
        Ok(r) => r,
        Err(e) => {
            warn!("failed to parse workflow_run data: {e}");
            return StatusCode::OK;
        }
    };
    let conclusion = run.conclusion.unwrap_or_else(|| "unknown".to_string());
    if conclusion != "failure" {
        info!("ignoring workflow_run with conclusion: {conclusion}");
        return StatusCode::OK;
    }
    info!(
        "GitHub workflow_run failed: {repo} workflow={} branch={}",
        run.name, run.head_branch
    );
    post_group(
        state,
        &cards::workflow_failed(
            repo,
            &run.name,
            &run.head_branch,
            &run.actor.login,
            &run.html_url,
        ),
    )
    .await;
    StatusCode::OK
}

async fn handle_secret_scanning(
    state: &Arc<AppState>,
    repo: &str,
    payload: octocrab::models::webhook_events::payload::SecretScanningAlertWebhookEventPayload,
) -> StatusCode {
    if payload.action != SecretScanningAlertWebhookEventAction::Created {
        info!("ignoring secret_scanning_alert action");
        return StatusCode::OK;
    }
    let alert: SecretScanningAlertData = match serde_json::from_value(payload.alert) {
        Ok(a) => a,
        Err(e) => {
            warn!("failed to parse secret_scanning_alert data: {e}");
            return StatusCode::OK;
        }
    };
    let secret_type = alert
        .secret_type_display_name
        .as_deref()
        .unwrap_or(&alert.secret_type);
    info!("GitHub secret scanning alert: {repo} type={secret_type}");
    post_group(
        state,
        &cards::secret_scanning(repo, secret_type, &alert.html_url),
    )
    .await;
    StatusCode::OK
}

async fn handle_dependabot(
    state: &Arc<AppState>,
    repo: &str,
    payload: octocrab::models::webhook_events::payload::DependabotAlertWebhookEventPayload,
) -> StatusCode {
    if payload.action != DependabotAlertWebhookEventAction::Created {
        info!("ignoring dependabot_alert action");
        return StatusCode::OK;
    }
    let alert: DependabotAlertData = match serde_json::from_value(payload.alert) {
        Ok(a) => a,
        Err(e) => {
            warn!("failed to parse dependabot_alert data: {e}");
            return StatusCode::OK;
        }
    };
    let severity = alert.severity.to_lowercase();
    if severity != "critical" && severity != "high" {
        info!("ignoring dependabot_alert with severity: {severity}");
        return StatusCode::OK;
    }
    let package = alert
        .dependency
        .as_ref()
        .and_then(|d| d.package.as_ref())
        .map(|p| p.name.as_str())
        .unwrap_or("unknown");
    let summary = alert
        .security_advisory
        .as_ref()
        .map(|a| a.summary.as_str())
        .unwrap_or("No summary available");
    info!("GitHub dependabot alert: {repo} pkg={package} severity={severity}");
    post_group(
        state,
        &cards::dependabot(repo, package, &severity, summary, &alert.html_url),
    )
    .await;
    StatusCode::OK
}
