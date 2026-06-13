use std::sync::Arc;

use async_trait::async_trait;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(GitHubApp)
}

struct GitHubApp;

#[async_trait]
impl App for GitHubApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "github".into(),
            kind: Kind::Integration,
            description: "GitHub webhook → Lark notification integration".into(),
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
        if state.github.webhook_secret.is_empty() {
            anyhow::bail!(
                "github webhook_secret is required (set [github].webhook_secret or GITHUB_WEBHOOK_SECRET)"
            );
        }
        Ok(Arc::new(GitHubInstance {
            state: Arc::new(state),
        }))
    }
}

struct GitHubInstance {
    state: Arc<AppState>,
}

#[async_trait]
impl Instance for GitHubInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        crate::run::serve(self.state.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.state, action, params).await
    }
}
