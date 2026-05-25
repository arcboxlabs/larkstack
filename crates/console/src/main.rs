use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
    routing::get,
};
use control::{ControlLayer, ControlPlane, EventStore};
use futures_util::stream::{Stream, StreamExt};
use linear_bridge::config::AppState as LinearBridgeState;
use serde::Deserialize;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;

const BACKFILL_LIMIT: usize = 200;

#[derive(Clone)]
struct ConsoleState {
    control: ControlPlane,
    store: EventStore,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir =
        PathBuf::from(std::env::var("CONSOLE_DATA_DIR").unwrap_or_else(|_| "data".to_string()));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;
    let store = EventStore::open(data_dir.join("events.db"))?;

    let control = ControlPlane::new();
    if let Some(max) = store.max_id().await? {
        control.advance_event_id(max);
    }

    init_tracing(&control);
    spawn_persistence(&control, &store);
    spawn_linear_bridge(&control);

    let state = ConsoleState {
        control: control.clone(),
        store: store.clone(),
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

fn spawn_persistence(control: &ControlPlane, store: &EventStore) {
    let mut rx = control.subscribe();
    let store = store.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let Err(e) = store.persist(ev).await {
                        warn!("event persist failed: {e:#}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("event persister lagged, dropped {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
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

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<u64>,
}

async fn events(
    State(s): State<ConsoleState>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    // Subscribe BEFORE the backfill query so live events that fire during the
    // SQLite read are buffered, not lost.
    let live_rx = s.control.subscribe();

    let since = q.since.or_else(|| {
        headers
            .get(header::HeaderName::from_static("last-event-id"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
    });

    let backfill = match since {
        Some(id) => s.store.since(id, BACKFILL_LIMIT).await.unwrap_or_default(),
        None => s.store.recent(BACKFILL_LIMIT).await.unwrap_or_default(),
    };
    let last_replayed = backfill.last().map(|e| e.id).unwrap_or(0);

    let history = futures_util::stream::iter(backfill.into_iter().map(|ev| {
        Ok::<_, Infallible>(
            SseEvent::default()
                .id(ev.id.to_string())
                .json_data(&ev)
                .unwrap_or_else(|_| SseEvent::default()),
        )
    }));

    let live = BroadcastStream::new(live_rx).filter_map(move |res| {
        let item = match res {
            Ok(ev) => {
                if ev.id <= last_replayed {
                    None // dropped: already delivered via backfill
                } else {
                    Some(Ok(SseEvent::default()
                        .id(ev.id.to_string())
                        .json_data(&ev)
                        .unwrap_or_else(|_| SseEvent::default())))
                }
            }
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                Some(Ok(SseEvent::default().event("lagged").data(n.to_string())))
            }
        };
        async move { item }
    });

    Sse::new(history.chain(live)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
