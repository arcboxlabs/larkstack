//! The App plug-in contract.
//!
//! An [`App`] is a registered, long-lived descriptor: it knows its identity and
//! can build a fresh, config-bound [`Instance`]. The host supervises instances —
//! it builds one per config generation, drives [`Instance::run`] and
//! [`Instance::handle_action`] concurrently, and owns status, cancellation, and
//! restart. Apps live in `apps/` and depend only on this contract.

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use sea_orm_migration::MigrationTrait;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{MetricsSink, StateStore};

/// Which family an App belongs to. Drives UI grouping and the supervisor's stop
/// strategy: Integrations are stateless and hard-abort-safe; Automations hold
/// in-flight multi-step flows and rely on cooperative cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Integration,
    Automation,
}

/// An App's self-description, served at `/api/apps` so the console can render
/// each App's grouping, status, and action controls generically.
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    pub name: String,
    pub kind: Kind,
    pub description: String,
    #[serde(default)]
    pub actions: Vec<ActionSpec>,
}

/// One action an App exposes. `params` is a free-form JSON descriptor the UI
/// renders a form from; `Null` means the action takes no parameters.
#[derive(Debug, Clone, Serialize)]
pub struct ActionSpec {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

impl ActionSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            params: Value::Null,
        }
    }

    pub fn with_params(mut self, params: Value) -> Self {
        self.params = params;
        self
    }
}

/// Framework capabilities handed to an App at build time. Cheap to clone (all
/// `Arc`/pooled handles). An App namespaces its [`StateStore`] keys, and its
/// [`db`](Self::db) tables, under its own name.
#[derive(Clone)]
pub struct AppServices {
    pub state: Arc<dyn StateStore>,
    pub metrics: Arc<dyn MetricsSink>,
    /// Shared relational store for App-owned tables. Tables an App creates via
    /// [`App::migrations`] are namespaced `"<app>_"`; see [`crate::db`].
    pub db: DatabaseConnection,
}

/// A registered App: a long-lived descriptor that builds a config-bound
/// [`Instance`] on demand.
#[async_trait]
pub trait App: Send + Sync + 'static {
    /// Static identity and self-description.
    fn manifest(&self) -> Manifest;

    /// Build a running instance from the current console config (full TOML; the
    /// App reads its own `[name]` section, overlaying env as it sees fit) plus
    /// the framework [`AppServices`]. Called on every (re)start. An `Err` marks
    /// the App errored in the console.
    async fn build(&self, config: &str, services: AppServices)
    -> anyhow::Result<Arc<dyn Instance>>;

    /// Schema migrations for this App's tables in the shared App database. Run
    /// by the framework before [`build`](Self::build); every table created must
    /// be namespaced `"<app>_"` (see [`crate::db`]). Empty by default.
    fn migrations(&self) -> Vec<Box<dyn MigrationTrait>> {
        Vec::new()
    }

    /// Optional admin/API routes the host mounts under `/api/apps/<name>/`,
    /// behind the same session gate as the rest of `/api`. The returned router
    /// carries its own state (typically [`AppServices::db`]); it is mounted once
    /// at startup and is independent of config reloads. `None` by default.
    fn routes(&self, services: &AppServices) -> Option<axum::Router> {
        let _ = services;
        None
    }

    /// Optional public inbound routes (webhooks, Lark event callbacks) the host
    /// mounts under `/webhooks/<name>/`, **outside** the OAuth session gate —
    /// callers are external systems authenticating with their own HMAC/token, not
    /// a console session. Mounted once at startup, so the router must read its
    /// config-built state at request time rather than capture it: pair a
    /// [`lark_kit`-style `StateSlot`] held on the App descriptor with the live
    /// instance, which publishes into it on `run`. `None` by default.
    ///
    /// [`lark_kit`-style `StateSlot`]: this crate stays Lark-agnostic, so the slot
    /// type lives in `lark-kit`; the host only nests the returned router.
    fn ingress_routes(&self, services: &AppServices) -> Option<axum::Router> {
        let _ = services;
        None
    }
}

/// A live, config-bound App instance. The host drives both methods concurrently.
#[async_trait]
pub trait Instance: Send + Sync {
    /// Long-running main loop. Must return promptly once `cancel` fires
    /// (cooperative shutdown). Returning `Err` marks the App errored; the
    /// supervisor restarts it with backoff.
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()>;

    /// Handle one console-dispatched action, concurrently with [`Instance::run`].
    /// The `Ok` string is surfaced as the action's result in the event stream;
    /// an `Err` as a failure. Unknown actions return `Err`.
    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        let _ = params;
        anyhow::bail!("app has no action '{action}'")
    }
}
