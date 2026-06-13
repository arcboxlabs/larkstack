use std::sync::Arc;

use larkstack_core::ActionEnvelope;
use serde::Deserialize;
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

pub async fn handle_actions(
    pipeline: Arc<Pipeline>,
    mut rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) {
    while let Some(env) = rx.recv().await {
        match env.name.as_str() {
            "process-meeting" => match serde_json::from_value::<ProcessMeetingParams>(env.params) {
                Ok(p) => match pipeline
                    .process_meeting(&p.meeting_id, p.owner.as_deref(), p.url.as_deref())
                    .await
                {
                    Ok(out) => info!(
                        action = "process-meeting",
                        meeting_id = %out.meeting_id,
                        doc_url = %out.doc_url,
                        segments = out.segments,
                        "ok"
                    ),
                    Err(e) => warn!(action = "process-meeting", "failed: {e:#}"),
                },
                Err(e) => warn!(
                    action = "process-meeting",
                    "invalid params (need meeting_id): {e}"
                ),
            },
            other => warn!(action = other, "unknown action"),
        }
    }
}
