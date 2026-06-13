use std::sync::Arc;

use async_trait::async_trait;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(XApp)
}

struct XApp;

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
        }))
    }
}

struct XInstance {
    state: Arc<AppState>,
}

#[async_trait]
impl Instance for XInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        crate::run::serve(self.state.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, _params: Value) -> anyhow::Result<String> {
        match action {
            "ping" => Ok("pong".into()),
            other => anyhow::bail!("unknown action '{other}'"),
        }
    }
}
