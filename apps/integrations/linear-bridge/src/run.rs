use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    routing::{get, post},
};
use control::ControlHandle;
use tracing::info;

use crate::config::AppState;

async fn health() -> &'static str {
    "ok"
}

pub async fn run(state: Arc<AppState>, handle: ControlHandle) -> anyhow::Result<()> {
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
    handle.running().await;

    let serve = axum::serve(listener, app);
    if let Err(e) = serve.await {
        handle.errored(format!("server error: {e}")).await;
        return Err(e.into());
    }
    Ok(())
}
