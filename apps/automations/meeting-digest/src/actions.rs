use std::sync::Arc;

use anyhow::Context;
use larkstack_core::ActionEnvelope;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::pipeline::Pipeline;

#[derive(Deserialize)]
struct ProcessMeetingParams {
    meeting_id: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Handle one action, returning a human-readable result. Shared by the embedded
/// App instance and the legacy drain loop.
pub async fn handle(pipeline: &Pipeline, action: &str, params: Value) -> anyhow::Result<String> {
    match action {
        "process-meeting" => {
            let p: ProcessMeetingParams =
                serde_json::from_value(params).context("invalid params (need meeting_id)")?;
            let out = pipeline
                .process_meeting(&p.meeting_id, p.owner.as_deref(), p.url.as_deref())
                .await
                .map_err(|e| anyhow::anyhow!("{e:#}"))?;
            Ok(format!(
                "processed {} → {} ({} segments)",
                out.meeting_id, out.doc_url, out.segments
            ))
        }
        other => anyhow::bail!("unknown action '{other}'"),
    }
}

/// Legacy drain loop (standalone supervisor path); logs each result.
pub async fn handle_actions(
    pipeline: Arc<Pipeline>,
    mut rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) {
    while let Some(env) = rx.recv().await {
        match handle(&pipeline, &env.name, env.params).await {
            Ok(msg) => info!(action = %env.name, "{msg}"),
            Err(e) => warn!(action = %env.name, "{e:#}"),
        }
    }
}
