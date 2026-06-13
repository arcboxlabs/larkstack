//! Shared control-plane types.
//!
//! `ControlPlane` is a single in-process bus:
//!
//! - **Status** — each subsystem reports liveness through its `ControlHandle`.
//!   The console reads a snapshot via [`ControlPlane::snapshot`].
//! - **Events** — a `tokio::sync::broadcast` channel multiplexes tracing
//!   records to any subscriber (the console's SSE handler is the first
//!   consumer). The [`tracing_layer`] feeds the bus automatically; subsystem
//!   code does not need to change.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast, mpsc, watch};

pub mod app;
mod metrics;
mod state;
mod store;
mod tracing_layer;

pub use app::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
pub use metrics::{Metric, MetricsSink, SqliteMetricsSink};
pub use state::{SqliteStateStore, StateStore};
pub use store::EventStore;
pub use tracing_layer::ControlLayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Starting,
    Running,
    Errored,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub state: State,
    pub message: Option<String>,
    #[serde(with = "ts_millis")]
    pub updated_at: SystemTime,
}

impl Status {
    pub fn new(state: State, message: Option<String>) -> Self {
        Self {
            state,
            message,
            updated_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<tracing::Level> for Level {
    fn from(l: tracing::Level) -> Self {
        match l {
            tracing::Level::TRACE => Level::Trace,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::INFO => Level::Info,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::ERROR => Level::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub id: u64,
    pub level: Level,
    pub subsystem: Option<String>,
    pub target: String,
    pub message: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
    #[serde(with = "ts_millis")]
    pub timestamp: SystemTime,
}

const EVENT_BUFFER: usize = 512;

#[derive(Clone)]
pub struct ControlPlane {
    inner: Arc<Inner>,
}

struct Inner {
    statuses: RwLock<HashMap<String, Status>>,
    events: broadcast::Sender<Event>,
    next_id: AtomicU64,
    config_tx: watch::Sender<Arc<String>>,
    actions: RwLock<HashMap<String, mpsc::UnboundedSender<ActionEnvelope>>>,
}

impl Default for ControlPlane {
    fn default() -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (config_tx, _) = watch::channel(Arc::new(String::new()));
        Self {
            inner: Arc::new(Inner {
                statuses: RwLock::new(HashMap::new()),
                events,
                next_id: AtomicU64::new(1),
                config_tx,
                actions: RwLock::new(HashMap::new()),
            }),
        }
    }
}

impl ControlPlane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&self, subsystem: impl Into<String>) -> ControlHandle {
        ControlHandle {
            subsystem: subsystem.into(),
            plane: self.clone(),
        }
    }

    pub async fn snapshot(&self) -> HashMap<String, Status> {
        self.inner.statuses.read().await.clone()
    }

    /// Subscribe to the event broadcast. Slow consumers may observe
    /// `RecvError::Lagged` and lose messages older than the buffer.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Emit a control-plane event. Build the event with [`Self::next_event_id`]
    /// so id ordering matches emission order.
    pub fn emit(&self, event: Event) {
        // Errors only happen when no receivers are listening; that's fine.
        let _ = self.inner.events.send(event);
    }

    pub fn next_event_id(&self) -> u64 {
        self.inner.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Ensure subsequent ids exceed the supplied watermark. Useful after
    /// loading the previous max id from [`EventStore`] at startup.
    pub fn advance_event_id(&self, min: u64) {
        self.inner.next_id.fetch_max(min + 1, Ordering::Relaxed);
    }

    /// Replace the current config TOML; subscribers receive the new value
    /// through [`Self::watch_config`].
    pub fn set_config(&self, toml: Arc<String>) {
        // send_replace ignores receiver count (always succeeds).
        self.inner.config_tx.send_replace(toml);
    }

    /// Read-only handle to the current config TOML.
    pub fn config(&self) -> Arc<String> {
        self.inner.config_tx.borrow().clone()
    }

    /// Subscribe to config changes. The receiver starts already-pending so
    /// callers can `borrow_and_update` on first poll.
    pub fn watch_config(&self) -> watch::Receiver<Arc<String>> {
        self.inner.config_tx.subscribe()
    }

    /// Register an action sink for `subsystem`. Replaces any prior registration
    /// (e.g. when the subsystem is restarted by the supervisor); dropping the
    /// returned receiver effectively un-registers the subsystem.
    pub async fn register_actions(
        &self,
        subsystem: &str,
    ) -> mpsc::UnboundedReceiver<ActionEnvelope> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner
            .actions
            .write()
            .await
            .insert(subsystem.to_string(), tx);
        rx
    }

    /// Send an action envelope to the named subsystem. Fails if the subsystem
    /// has never registered or its receiver has been dropped.
    pub async fn dispatch(
        &self,
        subsystem: &str,
        envelope: ActionEnvelope,
    ) -> Result<(), DispatchError> {
        let actions = self.inner.actions.read().await;
        let tx = actions
            .get(subsystem)
            .ok_or(DispatchError::UnknownSubsystem)?;
        tx.send(envelope).map_err(|_| DispatchError::Closed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub name: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("unknown subsystem")]
    UnknownSubsystem,
    #[error("subsystem not running")]
    Closed,
}

#[derive(Clone)]
pub struct ControlHandle {
    subsystem: String,
    plane: ControlPlane,
}

impl ControlHandle {
    pub fn subsystem(&self) -> &str {
        &self.subsystem
    }

    pub async fn set_status(&self, status: Status) {
        self.plane
            .inner
            .statuses
            .write()
            .await
            .insert(self.subsystem.clone(), status);
    }

    pub async fn running(&self) {
        self.set_status(Status::new(State::Running, None)).await;
    }

    pub async fn starting(&self) {
        self.set_status(Status::new(State::Starting, None)).await;
    }

    pub async fn stopped(&self) {
        self.set_status(Status::new(State::Stopped, None)).await;
    }

    pub async fn errored(&self, msg: impl Into<String>) {
        self.set_status(Status::new(State::Errored, Some(msg.into())))
            .await;
    }
}

mod ts_millis {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Serializer;

    pub fn serialize<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let ms = t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        s.serialize_i64(ms)
    }
}
