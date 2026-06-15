use std::sync::Arc;

use async_trait::async_trait;
use lark_kit::{SlotGuard, StateSlot};
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(GitHubApp {
        slot: lark_kit::slot(),
    })
}

struct GitHubApp {
    /// Live state cell shared by the host-mounted ingress router (read side) and
    /// each running [`GitHubInstance`] (write side); lives for the app's lifetime.
    slot: StateSlot<AppState>,
}

#[async_trait]
impl App for GitHubApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "github".into(),
            kind: Kind::Integration,
            description: "GitHub webhook → Lark notification integration".into(),
            actions: vec![
                ActionSpec::new("ping", "Log a pong — liveness check"),
                ActionSpec::new("test-notify", "Send a test card to a chat_id or DM email")
                    .with_params(
                        serde_json::json!({ "kind": "chat|dm", "target": "chat_id or email" }),
                    ),
            ],
        }
    }

    async fn build(
        &self,
        config: &str,
        services: AppServices,
    ) -> anyhow::Result<Arc<dyn Instance>> {
        let state = AppState::from_toml(config, services.state)
            .map_err(|e| anyhow::anyhow!("config: {e}"))?;
        if state.github.webhook_secret.is_empty() {
            anyhow::bail!(
                "github webhook_secret is required (set [github].webhook_secret or GITHUB_WEBHOOK_SECRET)"
            );
        }
        Ok(Arc::new(GitHubInstance {
            state: Arc::new(state),
            slot: self.slot.clone(),
        }))
    }

    /// Console admin routes: GET/PUT the live notification routing config.
    fn routes(&self, services: &AppServices) -> Option<axum::Router> {
        Some(lark_kit::routing::RoutingApi::new(services.state.clone(), "github").router())
    }

    fn ingress_routes(&self, _services: &AppServices) -> Option<axum::Router> {
        Some(crate::routes::ingress_router(self.slot.clone()))
    }
}

struct GitHubInstance {
    state: Arc<AppState>,
    slot: StateSlot<AppState>,
}

#[async_trait]
impl Instance for GitHubInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        // Publish state for the host-mounted ingress router; the guard clears the
        // slot when this run ends (shutdown or crash). There is no own server —
        // webhooks are served on the console port — so just hold until cancelled.
        let _guard = SlotGuard::publish(self.slot.clone(), self.state.clone());
        cancel.cancelled().await;
        Ok(())
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.state, action, params).await
    }
}
