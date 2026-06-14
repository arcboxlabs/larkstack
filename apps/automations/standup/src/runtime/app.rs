use std::sync::Arc;

use async_trait::async_trait;
use larkoapi::LarkBotClient;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::config::AppConfig;
use crate::runtime::run::{build_bot, serve_with_bot};

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(StandupApp)
}

struct StandupApp;

#[async_trait]
impl App for StandupApp {
    fn manifest(&self) -> Manifest {
        let date = json!({ "date": "today | tomorrow | YYYY-MM-DD" });
        Manifest {
            name: "standup".into(),
            kind: Kind::Automation,
            description: "Daily standup reminder bot".into(),
            actions: vec![
                ActionSpec::new("announce", "Create and announce the standup doc")
                    .with_params(date.clone()),
                ActionSpec::new("ensure", "Ensure the standup doc exists")
                    .with_params(date.clone()),
                ActionSpec::new("remind", "Remind people who haven't filled it")
                    .with_params(date.clone()),
                ActionSpec::new("urgent", "Urgent reminder + in-app escalation")
                    .with_params(date.clone()),
                ActionSpec::new("check", "Report standup completion status").with_params(date),
                ActionSpec::new("urgent-user", "Escalate to a single user").with_params(json!({
                    "open_id": "string (required)",
                    "date": "today | tomorrow | YYYY-MM-DD"
                })),
            ],
        }
    }

    async fn build(
        &self,
        config: &str,
        _services: AppServices,
    ) -> anyhow::Result<Arc<dyn Instance>> {
        let cfg = AppConfig::from_toml(config).map_err(|e| anyhow::anyhow!("config: {e}"))?;
        let bot = build_bot(&cfg)?;
        Ok(Arc::new(StandupInstance { cfg, bot }))
    }
}

struct StandupInstance {
    cfg: AppConfig,
    bot: Arc<LarkBotClient>,
}

#[async_trait]
impl Instance for StandupInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        serve_with_bot(&self.cfg, self.bot.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::trigger::actions::handle(&self.cfg.standup, &self.bot, action, params).await
    }
}
