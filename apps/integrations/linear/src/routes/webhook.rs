//! Axum handler for `POST /webhook` — receives Linear webhook payloads,
//! normalizes them, and feeds issues through debounce and comments straight to
//! the Lark sink.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use tracing::{error, info, warn};

use crate::config::AppState;
use crate::domain::debounce::PendingUpdate;
use crate::domain::{IssueNotification, Priority};
use crate::lark::{cards, notify};
use crate::source::changes::build_change_fields;
use crate::source::payload::{Actor, CommentData, Issue, LinearPayload, UpdatedFrom};

/// Handles incoming Linear webhook requests.
///
/// 1. Verifies the `linear-signature` HMAC header.
/// 2. Deserializes the [`LinearPayload`].
/// 3. Debounces issue creates/updates; dispatches comments immediately.
pub async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(signature) = headers
        .get("linear-signature")
        .and_then(|v| v.to_str().ok())
    else {
        warn!("missing linear-signature header");
        return StatusCode::UNAUTHORIZED;
    };
    if !lark_kit::verify_hmac_sha256(&state.linear.webhook_secret, &body, signature) {
        warn!("invalid webhook signature");
        return StatusCode::UNAUTHORIZED;
    }

    let payload: LinearPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to parse payload: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    match (payload.kind.as_str(), payload.action.as_str()) {
        ("Issue", "create") => {
            let issue: Issue = match serde_json::from_value(payload.data.clone()) {
                Ok(i) => i,
                Err(e) => {
                    error!("failed to parse Issue data: {e}");
                    return StatusCode::BAD_REQUEST;
                }
            };
            info!(
                "queuing debounced Issue create – {} {}",
                issue.identifier, issue.title
            );
            let dm_email = issue.assignee.as_ref().and_then(|a| a.email.clone());
            let issue_id = issue.id.clone();
            let notif = issue_to_notification(&issue, &payload.url, vec![], true);
            schedule_debounce(&state, issue_id, notif, dm_email).await;
            StatusCode::OK
        }
        ("Issue", "update") => {
            let issue: Issue = match serde_json::from_value(payload.data.clone()) {
                Ok(i) => i,
                Err(e) => {
                    error!("failed to parse Issue data: {e}");
                    return StatusCode::BAD_REQUEST;
                }
            };
            let changes = build_change_fields(&issue, &payload.updated_from);
            info!(
                "queuing debounced Issue update – {} {} (changes: {})",
                issue.identifier,
                issue.title,
                if changes.is_empty() {
                    "none detected".to_string()
                } else {
                    changes.join(", ")
                }
            );
            let dm_email: Option<String> = payload.updated_from.as_ref().and_then(|uf| {
                serde_json::from_value::<UpdatedFrom>(uf.clone())
                    .ok()
                    .and_then(|uf| {
                        if uf.assignee_id.is_some() {
                            issue.assignee.as_ref().and_then(|a| a.email.clone())
                        } else {
                            None
                        }
                    })
            });
            let issue_id = issue.id.clone();
            let notif = issue_to_notification(&issue, &payload.url, changes, false);
            schedule_debounce(&state, issue_id, notif, dm_email).await;
            StatusCode::OK
        }
        ("Comment", "create") => {
            let comment: CommentData = match serde_json::from_value(payload.data.clone()) {
                Ok(c) => c,
                Err(e) => {
                    error!("failed to parse Comment data: {e}");
                    return StatusCode::BAD_REQUEST;
                }
            };
            let actor: Option<Actor> = payload
                .data
                .get("user")
                .and_then(|u| serde_json::from_value(u.clone()).ok());
            let identifier = comment
                .issue
                .as_ref()
                .map(|i| i.identifier.clone())
                .unwrap_or_else(|| "?".into());
            let issue_title = comment
                .issue
                .as_ref()
                .map(|i| i.title.clone())
                .unwrap_or_default();
            let author = actor.map(|a| a.name).unwrap_or_else(|| "Someone".into());
            info!("processing Comment create on {identifier}");
            notify::group(
                &state,
                &cards::comment_card(
                    &identifier,
                    &issue_title,
                    &author,
                    &comment.body,
                    &payload.url,
                ),
            )
            .await;
            StatusCode::OK
        }
        _ => {
            info!(
                "ignoring event: type={}, action={}",
                payload.kind, payload.action
            );
            StatusCode::OK
        }
    }
}

async fn schedule_debounce(
    state: &Arc<AppState>,
    issue_id: String,
    notif: IssueNotification,
    dm_email: Option<String>,
) {
    let cancel_rx = state
        .debounce
        .upsert(issue_id.clone(), notif, dm_email)
        .await;

    let state = Arc::clone(state);
    let delay = state.debounce_delay_ms;
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(delay)) => {
                if let Some(p) = state.debounce.take(&issue_id).await {
                    send_debounced(&state, p).await;
                }
            }
            _ = cancel_rx => {}
        }
    });
}

async fn send_debounced(state: &AppState, pending: PendingUpdate) {
    let kind = if pending.notif.is_create {
        "create"
    } else {
        "update"
    };
    let changes = if pending.notif.changes.is_empty() {
        "none".to_string()
    } else {
        pending.notif.changes.join(", ")
    };
    info!("sending debounced {kind} – changes: {changes}");

    notify::group(state, &cards::issue_card(&pending.notif)).await;
    if let Some(linear_email) = &pending.dm_email {
        // The webhook gives us the assignee's Linear email; resolve it to their
        // Lark email through the admin override table (no-op when they match).
        let lark_email = crate::db::user_map::resolve_lark_email(&state.db, linear_email).await;
        notify::dm(state, &lark_email, &cards::assign_dm(&pending.notif)).await;
    }
}

/// Converts a Linear [`Issue`] into an [`IssueNotification`].
fn issue_to_notification(
    issue: &Issue,
    url: &str,
    changes: Vec<String>,
    is_create: bool,
) -> IssueNotification {
    IssueNotification {
        is_create,
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        description: issue.description.clone(),
        status: issue.state.name.clone(),
        priority: Priority::from_linear(issue.priority),
        assignee: issue.assignee.as_ref().map(|a| a.name.clone()),
        url: url.to_string(),
        changes,
    }
}
