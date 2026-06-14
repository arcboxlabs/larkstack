use std::sync::Arc;

use async_trait::async_trait;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(LinearApp)
}

struct LinearApp;

#[async_trait]
impl App for LinearApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "linear".into(),
            kind: Kind::Integration,
            description: "Linear webhook → Lark notification integration".into(),
            actions: vec![
                ActionSpec::new("ping", "Log a pong — liveness check"),
                ActionSpec::new("test-lark", "Send a test message to the Lark group webhook"),
            ],
        }
    }

    async fn build(
        &self,
        config: &str,
        services: AppServices,
    ) -> anyhow::Result<Arc<dyn Instance>> {
        let state =
            AppState::from_toml(config, services.db).map_err(|e| anyhow::anyhow!("config: {e}"))?;
        Ok(Arc::new(LinearInstance {
            state: Arc::new(state),
        }))
    }

    fn migrations(&self) -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        crate::db::user_map::migrations()
    }

    fn routes(&self, services: &AppServices) -> Option<axum::Router> {
        Some(crate::db::user_map::router(services.db.clone()))
    }
}

struct LinearInstance {
    state: Arc<AppState>,
}

#[async_trait]
impl Instance for LinearInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        crate::routes::serve(self.state.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.state, action, params).await
    }
}
