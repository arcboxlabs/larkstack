//! Subsystem status + App-manifest endpoints.
//!
//! The wire structs below are typed mirrors of the `larkstack_core` control
//! types. The host maps core → wire explicitly (rather than re-exporting the
//! core types as `ToSchema`) so that `larkstack-core` — which every App depends
//! on — never has to pull in `utoipa`. A core field rename breaks the `From`
//! impls here at compile time, keeping the schema honest.

use std::collections::HashMap;
use std::time::UNIX_EPOCH;

use axum::{Json, extract::State};
use larkstack_core::{
    ActionSpec as CoreActionSpec, Kind as CoreKind, Manifest as CoreManifest, State as CoreState,
    Status as CoreStatus,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::HostState;

/// `GET /api/status` — latest liveness of every supervised subsystem.
#[utoipa::path(
    get, path = "/status", tag = "console",
    security(("session" = [])),
    responses((status = 200, description = "Per-subsystem status snapshot", body = StatusResponse)),
)]
pub(crate) async fn status(State(s): State<HostState>) -> Json<StatusResponse> {
    let snapshot = s.control.snapshot().await;
    Json(StatusResponse {
        subsystems: snapshot
            .into_iter()
            .map(|(name, status)| (name, status.into()))
            .collect(),
    })
}

/// `GET /api/apps` — registered App manifests, for generic UI rendering.
#[utoipa::path(
    get, path = "/apps", tag = "console",
    security(("session" = [])),
    responses((status = 200, description = "Registered app manifests", body = AppsResponse)),
)]
pub(crate) async fn apps(State(s): State<HostState>) -> Json<AppsResponse> {
    Json(AppsResponse {
        apps: s.manifests.iter().map(AppManifest::from).collect(),
    })
}

// ---- wire types ------------------------------------------------------------

/// Body of `GET /api/status`.
#[derive(Serialize, ToSchema)]
pub(crate) struct StatusResponse {
    /// Subsystem name → its latest status.
    subsystems: HashMap<String, SubsystemStatus>,
}

/// One subsystem's status — wire mirror of `larkstack_core::Status`.
#[derive(Serialize, ToSchema)]
pub(crate) struct SubsystemStatus {
    state: SubsystemState,
    message: Option<String>,
    /// Unix-epoch milliseconds of the last transition.
    updated_at: u64,
}

impl From<CoreStatus> for SubsystemStatus {
    fn from(s: CoreStatus) -> Self {
        Self {
            state: s.state.into(),
            message: s.message,
            updated_at: s
                .updated_at
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }
}

/// Wire mirror of `larkstack_core::State`.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubsystemState {
    Starting,
    Running,
    Errored,
    Stopped,
}

impl From<CoreState> for SubsystemState {
    fn from(s: CoreState) -> Self {
        match s {
            CoreState::Starting => Self::Starting,
            CoreState::Running => Self::Running,
            CoreState::Errored => Self::Errored,
            CoreState::Stopped => Self::Stopped,
        }
    }
}

/// Body of `GET /api/apps`.
#[derive(Serialize, ToSchema)]
pub(crate) struct AppsResponse {
    apps: Vec<AppManifest>,
}

/// Wire mirror of `larkstack_core::Manifest`.
#[derive(Serialize, ToSchema)]
pub(crate) struct AppManifest {
    name: String,
    kind: AppKind,
    description: String,
    actions: Vec<AppActionSpec>,
}

impl From<&CoreManifest> for AppManifest {
    fn from(m: &CoreManifest) -> Self {
        Self {
            name: m.name.clone(),
            kind: m.kind.into(),
            description: m.description.clone(),
            actions: m.actions.iter().map(AppActionSpec::from).collect(),
        }
    }
}

/// Wire mirror of `larkstack_core::Kind`.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AppKind {
    Integration,
    Automation,
}

impl From<CoreKind> for AppKind {
    fn from(k: CoreKind) -> Self {
        match k {
            CoreKind::Integration => Self::Integration,
            CoreKind::Automation => Self::Automation,
        }
    }
}

/// Wire mirror of `larkstack_core::ActionSpec`.
#[derive(Serialize, ToSchema)]
pub(crate) struct AppActionSpec {
    name: String,
    description: String,
    /// Free-form JSON descriptor the UI renders a form from; omitted when the
    /// action takes no parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    params: Option<serde_json::Value>,
}

impl From<&CoreActionSpec> for AppActionSpec {
    fn from(a: &CoreActionSpec) -> Self {
        Self {
            name: a.name.clone(),
            description: a.description.clone(),
            params: (!a.params.is_null()).then(|| a.params.clone()),
        }
    }
}
