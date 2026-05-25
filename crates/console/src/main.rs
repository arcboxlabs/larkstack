use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
    routing::get,
};
use control::{ControlLayer, ControlPlane};
use futures_util::stream::Stream;
use linear_bridge::config::AppState as LinearBridgeState;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;

#[derive(Clone)]
struct ConsoleState {
    control: ControlPlane,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let control = ControlPlane::new();
    init_tracing(&control);

    spawn_linear_bridge(&control);

    let state = ConsoleState {
        control: control.clone(),
    };

    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/events", get(events))
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

fn init_tracing(control: &ControlPlane) {
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(ControlLayer::new(control.clone()))
        .init();
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

async fn events(
    State(s): State<ConsoleState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = BroadcastStream::new(s.control.subscribe()).filter_map(|res| match res {
        Ok(ev) => Some(Ok(SseEvent::default()
            .id(ev.id.to_string())
            .json_data(&ev)
            .unwrap_or_else(|_| SseEvent::default()))),
        Err(BroadcastStreamRecvError::Lagged(n)) => {
            Some(Ok(SseEvent::default().event("lagged").data(n.to_string())))
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
