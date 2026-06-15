//! Axum handler for `POST /webhook` — receives Linear webhook payloads,
//! normalizes them, and feeds issues through debounce and comments straight to
//! the Lark sink.

use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use lark_kit::Live;
use lark_kit::card::LarkCard;
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
    Live(state): Live<AppState>,
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
            let notif = issue_to_notification(&issue, &payload.url, vec![], true, false);
            schedule_debounce(&state, issue_id, notif, dm_email, payload.actor_id()).await;
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
            let updated_from: Option<UpdatedFrom> = payload
                .updated_from
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let status_changed = updated_from.as_ref().is_some_and(|uf| uf.state.is_some());
            // DM the assignee only when this update reassigned the issue.
            let dm_email = updated_from.as_ref().and_then(|uf| {
                if uf.assignee_id.is_some() {
                    issue.assignee.as_ref().and_then(|a| a.email.clone())
                } else {
                    None
                }
            });
            let issue_id = issue.id.clone();
            let notif = issue_to_notification(&issue, &payload.url, changes, false, status_changed);
            schedule_debounce(&state, issue_id, notif, dm_email, payload.actor_id()).await;
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
            // The comment author is the actor to exclude from fan-out.
            let actor_id = actor
                .as_ref()
                .and_then(|a| a.id.clone())
                .or_else(|| payload.actor_id());
            let author = actor
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Someone".into());
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

            // DM each subscriber (needs the API key + the commented issue id).
            let settings = crate::db::settings::load(&state.db).await;
            if settings.subscriber_on_comment
                && let Some(issue_id) = &comment.issue_id
            {
                let card = cards::subscriber_comment_dm(
                    &identifier,
                    &issue_title,
                    &author,
                    &comment.body,
                    &payload.url,
                );
                notify_subscribers(&state, issue_id, actor_id.as_deref(), &card).await;
            }
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
    actor_id: Option<String>,
) {
    let cancel_rx = state
        .debounce
        .upsert(issue_id.clone(), notif, dm_email, actor_id)
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

    // Fan out to subscribers on status changes (or any update in broad mode).
    let settings = crate::db::settings::load(&state.db).await;
    let fan_out = (pending.notif.status_changed && settings.subscriber_on_status_change)
        || settings.subscriber_on_any_update;
    if fan_out {
        let card = cards::subscriber_issue_dm(&pending.notif);
        notify_subscribers(
            state,
            &pending.notif.issue_id,
            pending.actor_id.as_deref(),
            &card,
        )
        .await;
    }
}

/// Fetch an issue's subscribers and DM each the same card, resolved to their Lark
/// email — skipping the triggering actor, deactivated users, and those without an
/// email. No-op without the GraphQL API key (subscriber emails require it).
async fn notify_subscribers(
    state: &AppState,
    issue_id: &str,
    actor_id: Option<&str>,
    card: &LarkCard,
) {
    let Some(client) = &state.linear_client else {
        return;
    };
    let subs = match client.fetch_issue_subscribers(issue_id).await {
        Ok(info) => info.subscribers,
        Err(e) => {
            warn!("subscriber fetch failed for {issue_id}: {e}");
            return;
        }
    };

    let mut emails = Vec::new();
    for s in subs {
        if !s.active || s.email.is_empty() || Some(s.id.as_str()) == actor_id {
            continue;
        }
        emails.push(crate::db::user_map::resolve_lark_email(&state.db, &s.email).await);
    }
    if !emails.is_empty() {
        notify::dm_many(state, &emails, card).await;
    }
}

/// Converts a Linear [`Issue`] into an [`IssueNotification`].
fn issue_to_notification(
    issue: &Issue,
    url: &str,
    changes: Vec<String>,
    is_create: bool,
    status_changed: bool,
) -> IssueNotification {
    IssueNotification {
        is_create,
        status_changed,
        identifier: issue.identifier.clone(),
        issue_id: issue.id.clone(),
        title: issue.title.clone(),
        description: issue.description.clone(),
        status: issue.state.name.clone(),
        priority: Priority::from_linear(issue.priority),
        assignee: issue.assignee.as_ref().map(|a| a.name.clone()),
        url: url.to_string(),
        changes,
    }
}
