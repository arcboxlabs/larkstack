use std::sync::Arc;

use anyhow::Context;
use larkstack_core::ActionEnvelope;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::config::AppState;

/// Handle one action, returning a human-readable result. Shared by the embedded
/// App instance and the legacy drain loop.
pub async fn handle(state: &AppState, action: &str, _params: Value) -> anyhow::Result<String> {
    match action {
        "ping" => Ok("pong".into()),
        "test-lark" => send_test_message(state).await,
        other => anyhow::bail!("unknown action '{other}'"),
    }
}

/// Legacy drain loop (standalone supervisor path); logs each result to the
/// event stream.
pub async fn handle_actions(state: Arc<AppState>, mut rx: mpsc::UnboundedReceiver<ActionEnvelope>) {
    while let Some(env) = rx.recv().await {
        match handle(&state, &env.name, env.params).await {
            Ok(msg) => info!(action = %env.name, "{msg}"),
            Err(e) => warn!(action = %env.name, "{e:#}"),
        }
    }
}

async fn send_test_message(state: &AppState) -> anyhow::Result<String> {
    if state.lark.webhook_url.is_empty() {
        anyhow::bail!("LARK_WEBHOOK_URL not set");
    }
    let body = json!({
        "msg_type": "text",
        "content": { "text": "[larkstack-console] test message" },
    });
    let resp = state
        .http
        .post(&state.lark.webhook_url)
        .json(&body)
        .send()
        .await
        .context("send failed")?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(format!("sent ({}): {text}", status.as_u16()))
    } else {
        anyhow::bail!("{} {text}", status.as_u16())
    }
}
