use std::sync::Arc;

use async_trait::async_trait;
use lark_kit::{LarkBotClient, SlotGuard, StateSlot};
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(LinearApp {
        slot: lark_kit::slot(),
        bot_slot: lark_kit::slot(),
    })
}

struct LinearApp {
    /// Live state cell shared by the host-mounted ingress router (read side) and
    /// each running [`LinearInstance`] (write side); lives for the app's lifetime.
    slot: StateSlot<AppState>,
    /// Live bot cell read by the routing admin `/chats` route; published by the
    /// running instance.
    bot_slot: StateSlot<LarkBotClient>,
}

#[async_trait]
impl App for LinearApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "linear".into(),
            kind: Kind::Integration,
            description: "Linear webhook → Lark notification integration".into(),
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
        let state = AppState::from_toml(config, services.state, services.db)
            .map_err(|e| anyhow::anyhow!("config: {e}"))?;
        Ok(Arc::new(LinearInstance {
            state: Arc::new(state),
            slot: self.slot.clone(),
            bot_slot: self.bot_slot.clone(),
        }))
    }

    fn migrations(&self) -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        crate::db::migrations()
    }

    /// Console admin routes: the DB-backed user-map + behavior settings, plus the
    /// live notification routing config (rules → chats/DMs) and the bot's chat list.
    fn routes(&self, services: &AppServices) -> Option<axum::Router> {
        let db = services.db.clone();
        let routing = lark_kit::routing::RoutingApi::new(services.state.clone(), "linear")
            .router(self.bot_slot.clone());
        Some(
            crate::db::user_map::router(db.clone())
                .merge(crate::db::settings::router(db))
                .merge(routing),
        )
    }

    fn ingress_routes(&self, _services: &AppServices) -> Option<axum::Router> {
        Some(crate::routes::ingress_router(self.slot.clone()))
    }
}

struct LinearInstance {
    state: Arc<AppState>,
    slot: StateSlot<AppState>,
    bot_slot: StateSlot<LarkBotClient>,
}

#[async_trait]
impl Instance for LinearInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        // Publish state for the host-mounted ingress router; the guard clears the
        // slot when this run ends (shutdown or crash). The webhook server now
        // lives on the console port, so `run` only drives the reminder scheduler,
        // which honors `cancel`.
        let _guard = SlotGuard::publish(self.slot.clone(), self.state.clone());
        // Publish the bot for the routing admin `/chats` route, when one is configured.
        let _bot_guard = self
            .state
            .bot
            .clone()
            .map(|bot| SlotGuard::publish(self.bot_slot.clone(), bot));
        crate::scheduler::run_scheduler(self.state.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.state, action, params).await
    }
}
