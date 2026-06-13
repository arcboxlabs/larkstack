//! Axum handler for `POST /github/webhook` — receives GitHub webhook payloads,
//! converts them to [`Event`]s via octocrab's strongly-typed `WebhookEvent`
//! models, and dispatches immediately (no debounce).

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
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
    config::{AppState, GitHubConfig},
    dispatch,
    event::Event,
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

/// Handles incoming GitHub webhook requests.
///
/// 1. Verifies the `X-Hub-Signature-256` HMAC header.
/// 2. Filters by the repo whitelist.
/// 3. Routes the event via octocrab's `WebhookEvent`.
pub async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(github) = &state.github else {
        warn!("received GitHub webhook but GitHub source is not configured");
        return StatusCode::NOT_FOUND;
    };

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
            let event = Event::PrOpened {
                repo: repo.to_string(),
                number,
                title,
                author,
                head_branch: pr.head.ref_field.clone(),
                base_branch: pr.base.ref_field.clone(),
                additions: pr.additions.unwrap_or(0),
                deletions: pr.deletions.unwrap_or(0),
                url: html_url,
            };
            dispatch::dispatch_github(&event, state, None).await;
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
            let dm_email = reviewer_lark_id.clone();
            let event = Event::PrReviewRequested {
                repo: repo.to_string(),
                number,
                title,
                author,
                reviewer,
                reviewer_lark_id,
                url: html_url,
            };
            dispatch::dispatch_github(&event, state, dm_email.as_deref()).await;
            StatusCode::OK
        }
        PullRequestWebhookEventAction::Closed if pr.merged_at.is_some() => {
            let merged_by = pr
                .merged_by
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_else(|| author.clone());
            info!("GitHub PR merged: {repo}#{number} by {merged_by}");
            let event = Event::PrMerged {
                repo: repo.to_string(),
                number,
                title,
                author,
                merged_by,
                url: html_url,
            };
            dispatch::dispatch_github(&event, state, None).await;
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
    let number = issue.number;
    info!("GitHub issue labeled alert: {repo}#{number} label={label}");
    let event = Event::IssueLabeledAlert {
        repo: repo.to_string(),
        number,
        title: issue.title.clone(),
        label,
        author: issue.user.login.clone(),
        url: issue.html_url.to_string(),
    };
    dispatch::dispatch_github(&event, state, None).await;
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
    dispatch_workflow_run(state, repo, payload.workflow_run).await
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
    dispatch_secret_scanning(state, repo, payload.alert).await
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
    dispatch_dependabot(state, repo, payload.alert).await
}

async fn dispatch_workflow_run(
    state: &Arc<AppState>,
    repo: &str,
    value: serde_json::Value,
) -> StatusCode {
    let run: WorkflowRunData = match serde_json::from_value(value) {
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
    let event = Event::WorkflowRunFailed {
        repo: repo.to_string(),
        workflow_name: run.name,
        branch: run.head_branch,
        actor: run.actor.login,
        conclusion,
        url: run.html_url,
    };
    dispatch::dispatch_github(&event, state, None).await;
    StatusCode::OK
}

async fn dispatch_secret_scanning(
    state: &Arc<AppState>,
    repo: &str,
    value: serde_json::Value,
) -> StatusCode {
    let alert: SecretScanningAlertData = match serde_json::from_value(value) {
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
    let event = Event::SecretScanningAlert {
        repo: repo.to_string(),
        secret_type: secret_type.to_string(),
        url: alert.html_url,
    };
    dispatch::dispatch_github(&event, state, None).await;
    StatusCode::OK
}

async fn dispatch_dependabot(
    state: &Arc<AppState>,
    repo: &str,
    value: serde_json::Value,
) -> StatusCode {
    let alert: DependabotAlertData = match serde_json::from_value(value) {
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
    let event = Event::DependabotAlert {
        repo: repo.to_string(),
        package: package.to_string(),
        severity,
        summary: summary.to_string(),
        url: alert.html_url,
    };
    dispatch::dispatch_github(&event, state, None).await;
    StatusCode::OK
}
