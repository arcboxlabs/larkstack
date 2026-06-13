use anyhow::Context;
use serde_json::{Value, json};

use crate::config::AppState;

/// Handle one console-dispatched action, returning a human-readable result.
pub async fn handle(state: &AppState, action: &str, _params: Value) -> anyhow::Result<String> {
    match action {
        "ping" => Ok("pong".into()),
        "test-lark" => send_test_message(state).await,
        other => anyhow::bail!("unknown action '{other}'"),
    }
}

async fn send_test_message(state: &AppState) -> anyhow::Result<String> {
    if state.lark.webhook_url.is_empty() {
        anyhow::bail!("lark webhook_url not set");
    }
    let body = json!({
        "msg_type": "text",
        "content": { "text": "[larkstack-console] github test message" },
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
