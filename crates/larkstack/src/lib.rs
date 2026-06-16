//! larkstack framework host.
//!
//! [`Larkstack`] is the runtime: register [`App`]s, then [`Larkstack::run`]
//! boots a supervisor per app, the tracing→event bus, the SQLite event store,
//! and the axum admin API + embedded console UI. The host depends only on the
//! App contract, never on individual apps.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use larkstack_core::{
    App, AppServices, ControlLayer, ControlPlane, EventStore, Manifest, SqliteMetricsSink,
    SqliteStateStore,
};
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod assets;
mod routes;
mod supervisor;

const DEFAULT_CONFIG: &str = r#"# larkstack console config
#
# Each app owns a top-level section. `enabled` (default false) toggles whether
# the host runs it — flip it from the console. Values left empty fall back to
# the matching environment variable (LINEAR_*, LARK_*, GITHUB_*, GITLAB_*,
# X_BEARER_TOKEN, STT_*, DIGEST_*, STANDUP_*), so secrets can stay in the environment.
#
# Inbound integrations are served on the console port under /webhooks/<app>/:
# linear → /webhooks/linear/webhook + /webhooks/linear/lark/event, github →
# /webhooks/github/webhook, gitlab → /webhooks/gitlab/webhook, x →
# /webhooks/x/lark/event. No per-app ports.
#
# Lark credentials live in a shared registry. Register them here, or onboard
# them from the console's "Lark Apps" tab (which live-tests them). An app then
# binds to one by name with `lark_app = "<name>"` instead of repeating
# app_id/app_secret inline.
#
# [lark-apps.main]
# app_id = "cli_..."
# app_secret = "..."
# base_url = "https://open.larksuite.com"

# Console sign-in uses Lark OAuth. Bind one of the [lark-apps] above as the
# OAuth client; until then the console is OPEN (so you can reach this UI to set
# it up). Register the redirect URI <your-console-url>/auth/callback in the Lark
# app's security settings.
[console]
# lark_app = "main"             # which [lark-apps.<name>] signs users in
# admins = ["you@example.com"]  # allowlist (matches Lark email); empty = any tenant user
# redirect_uri = ""             # override the auto-derived <host>/auth/callback
# scope = ""                    # OAuth scopes (space-separated). Unset + admins set =>
#                               # request the full user_info identity set by default
#                               # (email/enterprise_email/user_id/mobile); set empty to
#                               # request none. Every scope requested must be granted on
#                               # the Lark app's Permission Management page (else err 20027).

[linear]
enabled = false
# lark_app = "main"            # bind to [lark-apps.main]; or set app_id/secret in [linear.lark]
# webhook_secret = ""          # Linear webhook `linear-signature` HMAC secret
# api_key = ""                 # Linear GraphQL key — enables issue link previews
# debounce_delay_ms = 5000     # issue-update coalescing window (ms)
[linear.lark]
# webhook_url = ""
# verification_token = ""      # token for the link-preview app (POST /webhooks/linear/lark/event)
# base_url = "https://open.larksuite.com"

# github/gitlab deliver notifications through the bot; routing (repo/project/group →
# group chat / DM), reviewer user-map and alert labels are configured live from each app's
# console tab (stored per-App, not here). config.toml carries only secrets + the bot binding.
[github]
enabled = false
# lark_app = "main"            # binds [lark-apps.<name>] for the notification bot
# webhook_secret = ""          # HMAC for X-Hub-Signature-256; unset = won't start
[github.lark]
# app_id = ""                  # bot creds (alternative to lark_app)
# app_secret = ""
# base_url = "https://open.larksuite.com"

[gitlab]
enabled = false
# lark_app = "main"            # binds [lark-apps.<name>] for the notification bot
# webhook_token = ""           # X-Gitlab-Token plaintext secret (POST /webhooks/gitlab/webhook)
# signing_secret = ""          # GitLab 19.1+ Standard Webhooks signing token (whsec_…)
[gitlab.lark]
# app_id = ""                  # bot creds (alternative to lark_app)
# app_secret = ""
# base_url = "https://open.larksuite.com"

[x]
enabled = false
# lark_app = "main"
[x.lark]
# verification_token = ""      # token for the X link-preview app (POST /webhooks/x/lark/event)
# encrypt_key = ""             # X app Encrypt Key; decrypts AES-256-CBC callbacks
# base_url = "https://open.larksuite.com"

[minutes]
enabled = false
# lark_app = "main"
[minutes.lark]
# app_id = ""
# app_secret = ""
# base_url = "https://open.larksuite.com"
[minutes.stt]
# provider = "whisper_api"  # or "whisper_cpp"
# language = "auto"
# whisper_api_base = "https://api.openai.com/v1"
# whisper_api_model = "whisper-1"
# whisper_api_key = ""
# whisper_cpp_model = ""
# whisper_cpp_threads = 0
[minutes.digest]
# folder_token = ""
# fallback_chat_id = ""
# work_dir = ""
# ffmpeg = "ffmpeg"

[standup]
enabled = false
# lark_app = "main"
[standup.lark]
# app_id = ""
# app_secret = ""
# base_url = "https://open.larksuite.com"
[standup.standup]
# enabled = false   # scheduler auto-fire — distinct from [standup] above
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
            db: larkstack_core::db::open(data_dir.join("apps.db")).await?,
        };

        // Run each app's migrations once at startup, so its tables exist even
        // before it is enabled (its admin routes may read them). An app whose
        // migration fails is skipped — not supervised, no routes — and left
        // Errored, so one bad migration can't take the whole console down.
        let mut app_routers: Vec<(String, axum::Router)> = Vec::new();
        let mut ingress_routers: Vec<(String, axum::Router)> = Vec::new();
        for app in &self.apps {
            let name = app.manifest().name;
            if let Err(e) =
                larkstack_core::db::run_migrations(&services.db, &name, app.migrations()).await
            {
                warn!(app = %name, "migration failed; app disabled: {e:#}");
                control
                    .handle(name)
                    .errored(format!("migration: {e:#}"))
                    .await;
                continue;
            }
            supervisor::supervise(control.clone(), app.clone(), services.clone());
            if let Some(router) = app.routes(&services) {
                app_routers.push((name.clone(), router));
            }
            if let Some(router) = app.ingress_routes(&services) {
                ingress_routers.push((name, router));
            }
        }
        let manifests: Vec<Manifest> = self.apps.iter().map(|a| a.manifest()).collect();

        let state = HostState {
            control: control.clone(),
            store: store.clone(),
            config_path: config_path.clone(),
            manifests: Arc::new(manifests),
            http: reqwest::Client::new(),
            key: routes::oauth::load_session_key(&data_dir),
        };

        if routes::oauth::resolve(&control.config()).is_none() {
            warn!(
                "Lark OAuth is not configured ([console].lark_app unset) — /api/* is \
                 OPEN. Bind a Lark app from the console to require sign-in."
            );
        }

        let router = routes::build(state, app_routers, ingress_routers);

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
    http: reqwest::Client,
    /// Signing key for the OAuth session/handshake cookies.
    key: Key,
}

impl FromRef<HostState> for Key {
    fn from_ref(state: &HostState) -> Self {
        state.key.clone()
    }
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
