use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    routing::{get, post},
};
use larkstack_core::ControlHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::AppState;

async fn health() -> &'static str {
    "ok"
}

/// Build the webhook router and serve it until `cancel` fires. Shared by the
/// embedded App instance and the standalone binary.
pub async fn serve(state: Arc<AppState>, cancel: CancellationToken) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", state.server.port);

    let app = Router::new()
        .route("/webhook", post(crate::sources::linear::webhook_handler))
        .route("/lark/event", post(crate::sinks::lark::lark_event_handler))
        .route("/health", get(health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    info!("linear-bridge listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move { cancel.cancelled().await })
        .await?;
    Ok(())
}

/// Standalone entrypoint with its own [`ControlHandle`]; serves until killed.
pub async fn run(state: Arc<AppState>, handle: ControlHandle) -> anyhow::Result<()> {
    handle.running().await;
    serve(state, CancellationToken::new()).await
}
