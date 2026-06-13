use std::sync::Arc;

use larkstack_core::ActionEnvelope;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::config::AppState;

/// Drain action envelopes addressed to `linear-bridge`. Each handler emits
/// tracing events; the console surfaces them through SSE so success/failure
/// is visible in the UI without an explicit response channel.
pub async fn handle_actions(state: Arc<AppState>, mut rx: mpsc::UnboundedReceiver<ActionEnvelope>) {
    while let Some(env) = rx.recv().await {
        match env.name.as_str() {
            "ping" => info!(action = "ping", "pong"),
            "test-lark" => send_test_message(&state).await,
            other => warn!(action = other, "unknown action"),
        }
    }
}

async fn send_test_message(state: &AppState) {
    if state.lark.webhook_url.is_empty() {
        warn!(action = "test-lark", "skipped: LARK_WEBHOOK_URL not set");
        return;
    }
    let body = serde_json::json!({
        "msg_type": "text",
        "content": { "text": "[larkstack-console] test message" },
    });
    match state
        .http
        .post(&state.lark.webhook_url)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                info!(action = "test-lark", status = status.as_u16(), "{text}");
            } else {
                warn!(action = "test-lark", status = status.as_u16(), "{text}");
            }
        }
        Err(e) => warn!(action = "test-lark", "send failed: {e}"),
    }
}
