use std::sync::Arc;

use async_trait::async_trait;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(LinearBridgeApp)
}

struct LinearBridgeApp;

#[async_trait]
impl App for LinearBridgeApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "linear-bridge".into(),
            kind: Kind::Integration,
            description: "Linear + GitHub webhook → Lark notification bridge".into(),
            actions: vec![
                ActionSpec::new("ping", "Log a pong — liveness check"),
                ActionSpec::new("test-lark", "Send a test message to the Lark group webhook"),
            ],
        }
    }

    async fn build(
        &self,
        config: &str,
        _services: AppServices,
    ) -> anyhow::Result<Arc<dyn Instance>> {
        let state = AppState::from_toml(config).map_err(|e| anyhow::anyhow!("config: {e}"))?;
        Ok(Arc::new(LinearBridgeInstance {
            state: Arc::new(state),
        }))
    }
}

struct LinearBridgeInstance {
    state: Arc<AppState>,
}

#[async_trait]
impl Instance for LinearBridgeInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        crate::run::serve(self.state.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.state, action, params).await
    }
}
