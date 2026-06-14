//! WsEventHandler — listens on the Lark WebSocket long connection and fires
//! the pipeline on `vc.meeting.recording_ready_v1`.
//!
//! Note: this event only fires for meetings scheduled via `/vc/v1/reserves/apply`.
//! Ad-hoc meetings never emit it; for those, drive the pipeline via the CLI.

use std::sync::Arc;

use async_trait::async_trait;
use larkoapi::WsEventHandler;
use serde_json::Value;
use tokio::sync::Semaphore;
use tracing::{error, info};

use crate::pipeline::Pipeline;

pub struct RecordingReadyHandler {
    pub pipeline: Arc<Pipeline>,
    /// Cap on concurrent transcriptions.
    pub concurrency: Arc<Semaphore>,
}

impl RecordingReadyHandler {
    pub fn new(pipeline: Arc<Pipeline>, max_concurrent: usize) -> Self {
        Self {
            pipeline,
            concurrency: Arc::new(Semaphore::new(max_concurrent.max(1))),
        }
    }
}

#[async_trait]
impl WsEventHandler for RecordingReadyHandler {
    async fn handle_event(&self, event: &Value) -> Option<Value> {
        let event_type = event
            .pointer("/header/event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if event_type != "vc.meeting.recording_ready_v1" {
            return None;
        }

        let meeting_id = event
            .pointer("/event/meeting/id")
            .and_then(|v| v.as_str())
            .map(String::from);
        let topic = event
            .pointer("/event/meeting/topic")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let url = event
            .pointer("/event/url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let owner_open_id = event
            .pointer("/event/meeting/owner/id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let Some(meeting_id) = meeting_id else {
            error!("digest: recording_ready_v1 without meeting.id — payload={event}");
            return None;
        };

        info!(%meeting_id, %topic, "digest: recording_ready received");

        let pipeline = Arc::clone(&self.pipeline);
        let sem = Arc::clone(&self.concurrency);
        tokio::spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };
            if let Err(e) = pipeline
                .process_meeting(&meeting_id, owner_open_id.as_deref(), url.as_deref())
                .await
            {
                error!(%meeting_id, "digest: pipeline failed: {e}");
            }
        });

        None
    }
}
