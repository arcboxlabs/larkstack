use std::sync::Arc;

use anyhow::Context;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use control::ControlPlane;
use linear_bridge::config::AppState as LinearBridgeState;
use tracing::{error, info};

mod assets;

#[derive(Clone)]
struct ConsoleState {
    control: ControlPlane,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let control = ControlPlane::new();

    spawn_linear_bridge(&control);

    let state = ConsoleState {
        control: control.clone(),
    };

    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/health", get(|| async { "ok" }))
        .fallback(assets::serve)
        .with_state(state);

    let port: u16 = std::env::var("CONSOLE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    info!("console listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn spawn_linear_bridge(control: &ControlPlane) {
    let handle = control.handle("linear-bridge");
    tokio::spawn(async move {
        let state = Arc::new(LinearBridgeState::from_env());
        if let Err(e) = linear_bridge::run(state, handle.clone()).await {
            error!("linear-bridge stopped: {e:#}");
            handle.errored(format!("{e:#}")).await;
        }
    });
}

async fn status(State(s): State<ConsoleState>) -> (StatusCode, Json<serde_json::Value>) {
    let snap = s.control.snapshot().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "subsystems": snap })),
    )
}
