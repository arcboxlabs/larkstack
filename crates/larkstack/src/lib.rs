//! larkstack framework host.
//!
//! [`Larkstack`] is the runtime: register [`App`]s, then [`Larkstack::run`]
//! boots a supervisor per app, the tracing→event bus, the SQLite event store,
//! and the axum admin API + embedded console UI. The host depends only on the
//! App contract, never on individual apps.

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
use futures_util::stream::{Stream, StreamExt};
use larkstack_core::{
    ActionEnvelope, App, AppServices, ControlLayer, ControlPlane, DispatchError, EventStore,
    Manifest, SqliteMetricsSink, SqliteStateStore,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;
mod supervisor;

const BACKFILL_LIMIT: usize = 200;

const DEFAULT_CONFIG: &str = r#"# larkstack console config
#
# Each app owns a top-level section. `enabled` (default false) toggles whether
# the host runs it — flip it from the console. Values left empty fall back to
# the matching environment variable (LINEAR_*, LARK_*, STT_*, DIGEST_*,
# STANDUP_*, PORT, DEBOUNCE_DELAY_MS), so secrets can stay in the environment.

[linear-bridge]
enabled = false
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
enabled = false
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
enabled = false
[standup-bot.lark]
# app_id = ""
# app_secret = ""
# base_url = "https://open.larksuite.com"
[standup-bot.standup]
# enabled = false   # scheduler auto-fire — distinct from [standup-bot] above
# chat_id = ""
# folder_token = ""
"#;

/// The framework host. Register apps, then [`run`](Self::run).
#[derive(Default)]
pub struct Larkstack {
    apps: Vec<Arc<dyn App>>,
}

impl Larkstack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an App. Call once per app before [`run`](Self::run).
    pub fn register(mut self, app: Arc<dyn App>) -> Self {
        self.apps.push(app);
        self
    }

    /// Boot the host: supervise every registered app and serve the admin API +
    /// console UI until SIGINT/SIGTERM.
    pub async fn run(self) -> anyhow::Result<()> {
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

        let services = AppServices {
            state: Arc::new(SqliteStateStore::open(data_dir.join("state.db"))?),
            metrics: Arc::new(SqliteMetricsSink::open(data_dir.join("metrics.db"))?),
        };
        for app in &self.apps {
            supervisor::supervise(control.clone(), app.clone(), services.clone());
        }
        let manifests: Vec<Manifest> = self.apps.iter().map(|a| a.manifest()).collect();

        let state = HostState {
            control: control.clone(),
            store: store.clone(),
            config_path: config_path.clone(),
            manifests: Arc::new(manifests),
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
            .route("/apps", get(apps))
            .route("/events", get(events))
            .route("/config", get(get_config).put(put_config))
            .route(
                "/actions/{app}/{action}",
                axum::routing::post(dispatch_action),
            )
            .route_layer(auth_layer)
            .route("/health", get(|| async { "ok" }));

        let router = Router::new()
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
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        info!("console shut down cleanly");
        Ok(())
    }
}

#[derive(Clone)]
struct HostState {
    control: ControlPlane,
    store: EventStore,
    config_path: Arc<PathBuf>,
    manifests: Arc<Vec<Manifest>>,
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

async fn status(State(s): State<HostState>) -> (StatusCode, Json<serde_json::Value>) {
    let snap = s.control.snapshot().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "subsystems": snap })),
    )
}

async fn apps(State(s): State<HostState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "apps": &*s.manifests }))
}

async fn get_config(
    State(s): State<HostState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let toml = (*s.control.config()).clone();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/toml; charset=utf-8")],
        toml,
    )
}

async fn put_config(
    State(s): State<HostState>,
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
    info!("config updated; affected apps will restart");
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn dispatch_action(
    State(s): State<HostState>,
    AxPath((app, action)): AxPath<(String, String)>,
    body: Option<Json<serde_json::Value>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let params = body.map(|j| j.0).unwrap_or(serde_json::Value::Null);
    let envelope = ActionEnvelope {
        name: action.clone(),
        params,
    };
    match s.control.dispatch(&app, envelope).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "ok": true, "app": app, "action": action })),
        ),
        Err(DispatchError::UnknownSubsystem) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("unknown app '{app}'") })),
        ),
        Err(DispatchError::Closed) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": format!("app '{app}' is not running") })),
        ),
    }
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<u64>,
}

async fn events(
    State(s): State<HostState>,
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
