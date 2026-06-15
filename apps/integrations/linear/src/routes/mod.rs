//! The public inbound router (webhook + Lark preview callback), mounted by the
//! host at `/webhooks/linear/`.

use axum::{Router, routing::post};
use lark_kit::StateSlot;

use crate::config::AppState;

mod preview;
mod webhook;

/// Build the Linear ingress router. It reads its live [`AppState`] from `slot`
/// per request via the [`lark_kit::Live`] extractor (`503` while the app is
/// stopped), so the once-mounted router tracks config reloads.
pub fn ingress_router(slot: StateSlot<AppState>) -> Router {
    Router::new()
        .route("/webhook", post(webhook::webhook_handler))
        .route("/lark/event", post(preview::lark_event_handler))
        .with_state(slot)
}
