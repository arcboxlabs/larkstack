use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use larkstack_core::ControlHandle;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

async fn health() -> &'static str {
    "ok"
}

/// Build the event-callback router and serve it until `cancel` fires.
pub async fn serve(state: Arc<AppState>, cancel: CancellationToken) -> anyhow::Result<()> {
    let port = state.server.port;
    let app = Router::new()
        .route(
            "/lark/event",
            post(crate::event_handler::lark_event_handler),
        )
        .route("/health", get(health))
        .with_state(state);
    lark_kit::server::serve("x", app, port, cancel).await
}

/// Standalone entrypoint with its own [`ControlHandle`]; serves until killed.
pub async fn run(state: Arc<AppState>, handle: ControlHandle) -> anyhow::Result<()> {
    handle.running().await;
    serve(state, CancellationToken::new()).await
}
