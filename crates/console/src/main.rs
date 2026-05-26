use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::extract::Path as AxPath;
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{
        Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::get,
};
use control::{
    ActionEnvelope, ControlHandle, ControlLayer, ControlPlane, DispatchError, EventStore,
};
use futures_util::stream::{Stream, StreamExt};
use linear_bridge::config::AppState as LinearBridgeState;
use meeting_digest::AppConfig as MeetingDigestCfg;
use serde::Deserialize;
use standup_bot::AppConfig as StandupCfg;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;

const BACKFILL_LIMIT: usize = 200;

const DEFAULT_CONFIG: &str = r#"# larkstack-console config
#
# Each subsystem owns a top-level section. Values left empty fall back to the
# matching environment variable (LINEAR_*, LARK_*, STT_*, DIGEST_*, STANDUP_*,
# PORT, DEBOUNCE_DELAY_MS), so secrets can stay in the environment while ops
# fields are edited from the UI.

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

[meeting-digest]
[meeting-digest.lark]
# app_id = ""
# app_secret = ""
# base_url = "https://open.larksuite.com"
[meeting-digest.stt]
# provider = "whisper_api"  # or "whisper_cpp"
# language = "auto"
# whisper_api_base = "https://api.openai.com/v1"
# whisper_api_model = "whisper-1"
# whisper_api_key = ""
# whisper_cpp_model = ""
# whisper_cpp_threads = 0
[meeting-digest.digest]
# folder_token = ""
# fallback_chat_id = ""
# work_dir = ""
# ffmpeg = "ffmpeg"

[standup-bot]
[standup-bot.lark]
# app_id = ""
# app_secret = ""
# base_url = "https://open.larksuite.com"
[standup-bot.standup]
# enabled = false
# chat_id = ""
# folder_token = ""
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
    supervise(control.clone(), "linear-bridge", spawn_linear_bridge);
    supervise(control.clone(), "meeting-digest", spawn_meeting_digest);
    supervise(control.clone(), "standup-bot", spawn_standup_bot);

    let state = ConsoleState {
        control: control.clone(),
        store: store.clone(),
        config_path: config_path.clone(),
    };

    let token = std::env::var("CONSOLE_TOKEN").ok().map(Arc::new);
    if token.is_none() {
        warn!(
            "CONSOLE_TOKEN is unset — /api/* is OPEN. Set CONSOLE_TOKEN to a \
             random secret before exposing this console outside localhost."
        );
    }
    let auth_layer = middleware::from_fn(move |req, next| {
        let token = token.clone();
        async move { require_auth(token, req, next).await }
    });

    let api = Router::new()
        .route("/status", get(status))
        .route("/events", get(events))
        .route("/config", get(get_config).put(put_config))
        .route(
            "/actions/{subsystem}/{action}",
            axum::routing::post(dispatch_action),
        )
        .route_layer(auth_layer)
        .route("/health", get(|| async { "ok" }));

    let app = Router::new()
        .nest("/api", api)
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
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("console shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("ctrl-c received; shutting down"),
        _ = terminate => info!("SIGTERM received; shutting down"),
    }
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

/// Generic supervisor: parses TOML, spawns a subsystem task, restarts on every
/// config change (broadcast via `ControlPlane`). `spawn_task` is responsible
/// for parsing its own slice of the TOML, building state, and running the
/// subsystem; the supervisor only orchestrates lifecycle.
fn supervise<F>(control: ControlPlane, name: &'static str, spawn_task: F)
where
    F: Fn(
            Arc<String>,
            ControlHandle,
            mpsc::UnboundedReceiver<ActionEnvelope>,
        ) -> tokio::task::JoinHandle<()>
        + Send
        + 'static,
{
    let handle = control.handle(name);
    let mut config_rx = control.watch_config();
    tokio::spawn(async move {
        loop {
            let toml = (*config_rx.borrow_and_update()).clone();
            let action_rx = control.register_actions(name).await;
            let mut task = spawn_task(toml, handle.clone(), action_rx);
            tokio::select! {
                _ = config_rx.changed() => {
                    info!(subsystem = name, "config changed; restarting");
                    task.abort();
                    let _ = task.await;
                }
                res = &mut task => {
                    let _ = res;
                    if config_rx.changed().await.is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn spawn_linear_bridge(
    toml: Arc<String>,
    handle: ControlHandle,
    action_rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let state = match LinearBridgeState::from_toml(&toml) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                error!("linear-bridge config invalid: {e:#}");
                handle.errored(format!("config: {e:#}")).await;
                return;
            }
        };
        let state_for_actions = state.clone();
        let h = handle.clone();
        tokio::select! {
            _ = linear_bridge::handle_actions(state_for_actions, action_rx) => {}
            res = linear_bridge::run(state, h.clone()) => {
                if let Err(e) = res {
                    error!("linear-bridge stopped: {e:#}");
                    h.errored(format!("{e:#}")).await;
                }
            }
        }
    })
}

fn spawn_meeting_digest(
    toml: Arc<String>,
    handle: ControlHandle,
    action_rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let cfg = match MeetingDigestCfg::from_toml(&toml) {
            Ok(c) => c,
            Err(e) => {
                error!("meeting-digest config invalid: {e:#}");
                handle.errored(format!("config: {e:#}")).await;
                return;
            }
        };
        let pipeline = match meeting_digest::build_pipeline(&cfg) {
            Ok(p) => p,
            Err(e) => {
                error!("meeting-digest build failed: {e:#}");
                handle.errored(format!("{e:#}")).await;
                return;
            }
        };
        let actions_pipeline = pipeline.clone();
        tokio::select! {
            _ = meeting_digest::handle_actions(actions_pipeline, action_rx) => {}
            _ = meeting_digest::run_ws(&cfg, pipeline, handle.clone()) => {}
        }
    })
}

fn spawn_standup_bot(
    toml: Arc<String>,
    handle: ControlHandle,
    action_rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let cfg = match StandupCfg::from_toml(&toml) {
            Ok(c) => c,
            Err(e) => {
                error!("standup-bot config invalid: {e:#}");
                handle.errored(format!("config: {e:#}")).await;
                return;
            }
        };
        let bot = match standup_bot::build_bot(&cfg) {
            Ok(b) => b,
            Err(e) => {
                error!("standup-bot build failed: {e:#}");
                handle.errored(format!("{e:#}")).await;
                return;
            }
        };
        let standup_cfg = cfg.standup.clone();
        let actions_bot = bot.clone();
        tokio::select! {
            _ = standup_bot::handle_actions(standup_cfg, actions_bot, action_rx) => {}
            _ = standup_bot::run_with_bot(cfg, bot, handle.clone()) => {}
        }
    })
}

async fn require_auth(
    expected: Option<Arc<String>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(expected) = expected else {
        return Ok(next.run(req).await);
    };

    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let query_token = req
        .uri()
        .query()
        .and_then(|q| {
            q.split('&').find_map(|p| {
                let (k, v) = p.split_once('=')?;
                (k == "token").then_some(v)
            })
        })
        .map(urldecode);

    let provided = header_token.map(str::to_string).or(query_token);

    match provided {
        Some(p) if p.as_bytes().ct_eq(expected.as_bytes()).into() => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn urldecode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'%' => {
                let h = bytes.next();
                let l = bytes.next();
                if let (Some(h), Some(l)) = (h, l)
                    && let (Some(hi), Some(lo)) =
                        ((h as char).to_digit(16), (l as char).to_digit(16))
                {
                    out.push(((hi << 4 | lo) as u8) as char);
                } else {
                    out.push('%');
                }
            }
            b'+' => out.push(' '),
            other => out.push(other as char),
        }
    }
    out
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
