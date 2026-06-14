use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use larkstack_core::ControlHandle;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

mod preview;
mod webhook;

async fn health() -> &'static str {
    "ok"
}

/// Build the webhook + event router and serve it until `cancel` fires.
pub async fn serve(state: Arc<AppState>, cancel: CancellationToken) -> anyhow::Result<()> {
    let port = state.server.port;
    let app = Router::new()
        .route("/webhook", post(webhook::webhook_handler))
        .route("/lark/event", post(preview::lark_event_handler))
        .route("/health", get(health))
        .with_state(state);
    lark_kit::server::serve("linear", app, port, cancel).await
}

/// Standalone entrypoint with its own [`ControlHandle`]; runs the webhook server
/// and reminder scheduler until killed.
pub async fn run(state: Arc<AppState>, handle: ControlHandle) -> anyhow::Result<()> {
    handle.running().await;
    let cancel = CancellationToken::new();
    tokio::try_join!(
        serve(state.clone(), cancel.clone()),
        crate::scheduler::run_scheduler(state, cancel),
    )?;
    Ok(())
}
