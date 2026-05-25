//! Shared control-plane types.
//!
//! `ControlHandle` is cloned into each subsystem so it can report status back
//! to the console. Later phases will extend this with an event bus and an
//! action receiver, but phase 1 only needs status reporting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use serde::Serialize;
use tokio::sync::RwLock;

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

#[derive(Clone, Default)]
pub struct ControlPlane {
    statuses: Arc<RwLock<HashMap<String, Status>>>,
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
        self.statuses.read().await.clone()
    }
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
            .statuses
            .write()
            .await
            .insert(self.subsystem.clone(), status);
    }

    pub async fn running(&self) {
        self.set_status(Status::new(State::Running, None)).await;
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
