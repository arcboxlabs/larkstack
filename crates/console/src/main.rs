use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::extract::Path as AxPath;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
    routing::get,
};
use control::{ActionEnvelope, ControlLayer, ControlPlane, DispatchError, EventStore};
use futures_util::stream::{Stream, StreamExt};
use linear_bridge::config::AppState as LinearBridgeState;
use serde::Deserialize;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;

const BACKFILL_LIMIT: usize = 200;

const DEFAULT_CONFIG: &str = r#"# larkstack-console config
#
# Each subsystem owns a top-level section. Values left empty fall back to the
# matching environment variable (LINEAR_*, LARK_*, PORT, DEBOUNCE_DELAY_MS),
# so secrets can stay in the environment while the rest is edited from the UI.

[linear-bridge]

[linear-bridge.linear]
# webhook_secret = ""
# api_key = ""

[linear-bridge.lark]
# webhook_url = ""
# app_id = ""
# app_secret = ""
# verification_token = ""
# base_url = "https://open.larksuite.com"

[linear-bridge.server]
# port = 3000
# debounce_delay_ms = 5000
"#;

#[derive(Clone)]
struct ConsoleState {
    control: ControlPlane,
    store: EventStore,
    config_path: Arc<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir =
        PathBuf::from(std::env::var("CONSOLE_DATA_DIR").unwrap_or_else(|_| "data".to_string()));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;

    let store = EventStore::open(data_dir.join("events.db"))?;
    let config_path = Arc::new(data_dir.join("config.toml"));

    let control = ControlPlane::new();
    if let Some(max) = store.max_id().await? {
        control.advance_event_id(max);
    }

    let initial_config = load_or_create_config(&config_path)?;
    control.set_config(Arc::new(initial_config));

    init_tracing(&control);
    spawn_persistence(&control, &store);
    spawn_linear_bridge_supervisor(&control);

    let state = ConsoleState {
        control: control.clone(),
        store: store.clone(),
        config_path: config_path.clone(),
    };

    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/events", get(events))
        .route("/api/config", get(get_config).put(put_config))
        .route(
            "/api/actions/{subsystem}/{action}",
            axum::routing::post(dispatch_action),
        )
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

fn load_or_create_config(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        let s =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(s)
    } else {
        std::fs::write(path, DEFAULT_CONFIG)
            .with_context(|| format!("write {}", path.display()))?;
        info!("wrote default config to {}", path.display());
        Ok(DEFAULT_CONFIG.to_string())
    }
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

/// Runs `linear_bridge::run` in a loop, restarting whenever the config TOML
/// changes. Aborts the in-flight task on change; new state is built from the
/// updated TOML before respawn.
fn spawn_linear_bridge_supervisor(control: &ControlPlane) {
    let control = control.clone();
    let handle = control.handle("linear-bridge");
    let mut config_rx = control.watch_config();

    tokio::spawn(async move {
        loop {
            let toml = (*config_rx.borrow_and_update()).clone();
            let state = match LinearBridgeState::from_toml(&toml) {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    error!("linear-bridge config invalid: {e:#}");
                    handle.errored(format!("config: {e:#}")).await;
                    if config_rx.changed().await.is_err() {
                        break;
                    }
                    continue;
                }
            };

            let h = handle.clone();
            let action_rx = control.register_actions("linear-bridge").await;
            let state_for_actions = state.clone();
            let mut task = tokio::spawn(async move {
                tokio::select! {
                    _ = linear_bridge::handle_actions(state_for_actions, action_rx) => {}
                    res = linear_bridge::run(state, h.clone()) => {
                        if let Err(e) = res {
                            error!("linear-bridge stopped: {e:#}");
                            h.errored(format!("{e:#}")).await;
                        }
                    }
                }
            });

            tokio::select! {
                _ = config_rx.changed() => {
                    info!("config changed; restarting linear-bridge");
                    task.abort();
                    let _ = task.await;
                }
                res = &mut task => {
                    let _ = res;
                    // task exited on its own; wait for next config change before retry.
                    if config_rx.changed().await.is_err() {
                        break;
                    }
                }
            }
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

async fn get_config(
    State(s): State<ConsoleState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let toml = (*s.control.config()).clone();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/toml; charset=utf-8")],
        toml,
    )
}

async fn put_config(
    State(s): State<ConsoleState>,
    body: String,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = toml::from_str::<toml::Value>(&body) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("invalid TOML: {e}") })),
        );
    }
    if let Err(e) = std::fs::write(s.config_path.as_path(), &body) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("write {}: {e}", s.config_path.display()) })),
        );
    }
    s.control.set_config(Arc::new(body));
    info!("config updated; subsystems will restart");
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn dispatch_action(
    State(s): State<ConsoleState>,
    AxPath((subsystem, action)): AxPath<(String, String)>,
    body: Option<Json<serde_json::Value>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let params = body.map(|j| j.0).unwrap_or(serde_json::Value::Null);
    let envelope = ActionEnvelope {
        name: action.clone(),
        params,
    };
    match s.control.dispatch(&subsystem, envelope).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "ok": true, "subsystem": subsystem, "action": action })),
        ),
        Err(DispatchError::UnknownSubsystem) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("unknown subsystem '{subsystem}'") })),
        ),
        Err(DispatchError::Closed) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": format!("subsystem '{subsystem}' is not running") })),
        ),
    }
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
                    None
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
