use std::sync::Arc;

use async_trait::async_trait;
use lark_kit::{SlotGuard, StateSlot};
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(XApp {
        slot: lark_kit::slot(),
    })
}

struct XApp {
    /// Live state cell shared by the host-mounted ingress router (read side) and
    /// each running [`XInstance`] (write side); lives for the app's lifetime.
    slot: StateSlot<AppState>,
}

#[async_trait]
impl App for XApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "x".into(),
            kind: Kind::Integration,
            description: "X (Twitter) link-preview integration for Lark".into(),
            actions: vec![ActionSpec::new("ping", "Log a pong — liveness check")],
        }
    }

    async fn build(
        &self,
        config: &str,
        _services: AppServices,
    ) -> anyhow::Result<Arc<dyn Instance>> {
        let state = AppState::from_toml(config).map_err(|e| anyhow::anyhow!("config: {e}"))?;
        Ok(Arc::new(XInstance {
            state: Arc::new(state),
            slot: self.slot.clone(),
        }))
    }

    fn ingress_routes(&self, _services: &AppServices) -> Option<axum::Router> {
        Some(crate::routes::ingress_router(self.slot.clone()))
    }
}

struct XInstance {
    state: Arc<AppState>,
    slot: StateSlot<AppState>,
}

#[async_trait]
impl Instance for XInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        // Publish state for the host-mounted ingress router; the guard clears the
        // slot when this run ends. Preview-only — no own server or background
        // work — so just hold until cancelled.
        let _guard = SlotGuard::publish(self.slot.clone(), self.state.clone());
        cancel.cancelled().await;
        Ok(())
    }

    async fn handle_action(&self, action: &str, _params: Value) -> anyhow::Result<String> {
        match action {
            "ping" => Ok("pong".into()),
            other => anyhow::bail!("unknown action '{other}'"),
        }
    }
}
