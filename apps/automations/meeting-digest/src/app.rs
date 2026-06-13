use std::sync::Arc;

use async_trait::async_trait;
use larkstack_core::{ActionSpec, App, AppServices, Instance, Kind, Manifest};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::config::AppConfig;
use crate::pipeline::Pipeline;
use crate::run::{build_pipeline, serve_ws};

/// The registered App for the console host.
pub fn app() -> Arc<dyn App> {
    Arc::new(MeetingDigestApp)
}

struct MeetingDigestApp;

#[async_trait]
impl App for MeetingDigestApp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "meeting-digest".into(),
            kind: Kind::Automation,
            description: "Auto-transcribe Lark/Feishu recorded meetings and post digest cards"
                .into(),
            actions: vec![
                ActionSpec::new(
                    "process-meeting",
                    "Transcribe a meeting by ID and post its digest card",
                )
                .with_params(json!({
                    "meeting_id": "string (required)",
                    "owner": "string",
                    "url": "string"
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
        let pipeline = build_pipeline(&cfg)?;
        Ok(Arc::new(MeetingDigestInstance { cfg, pipeline }))
    }
}

struct MeetingDigestInstance {
    cfg: AppConfig,
    pipeline: Arc<Pipeline>,
}

#[async_trait]
impl Instance for MeetingDigestInstance {
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        serve_ws(&self.cfg, self.pipeline.clone(), cancel).await
    }

    async fn handle_action(&self, action: &str, params: Value) -> anyhow::Result<String> {
        crate::actions::handle(&self.pipeline, action, params).await
    }
}
