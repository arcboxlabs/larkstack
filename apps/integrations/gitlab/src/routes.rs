//! The public webhook router, mounted by the host at `/webhooks/gitlab/`.

use axum::{Router, routing::post};
use lark_kit::StateSlot;

use crate::config::AppState;

/// Build the GitLab ingress router. It reads its live [`AppState`] from `slot`
/// per request via the [`lark_kit::Live`] extractor (`503` while the app is
/// stopped), so the once-mounted router tracks config reloads.
pub fn ingress_router(slot: StateSlot<AppState>) -> Router {
    Router::new()
        .route("/webhook", post(crate::source::webhook_handler))
        .with_state(slot)
}
