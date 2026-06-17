use anyhow::{anyhow, bail};
use lark_kit::card::{card, md_div};
use serde::Deserialize;
use serde_json::Value;

use crate::config::AppState;

/// Handle one console-dispatched action, returning a human-readable result.
pub async fn handle(state: &AppState, action: &str, params: Value) -> anyhow::Result<String> {
    match action {
        "ping" => Ok("pong".into()),
        "test-notify" => test_notify(state, params).await,
        other => bail!("unknown action '{other}'"),
    }
}

#[derive(Deserialize)]
struct TestParams {
    kind: String,
    target: String,
}

/// Send a test card to a routing destination, surfacing the bot's send result.
async fn test_notify(state: &AppState, params: Value) -> anyhow::Result<String> {
    let p: TestParams = serde_json::from_value(params)
        .map_err(|e| anyhow!("params must be {{ kind: \"chat\"|\"dm\", target }}: {e}"))?;
    let bot = state
        .bot
        .as_ref()
        .ok_or_else(|| anyhow!("no Lark bot configured (bind [linear].lark_app)"))?;
    let test = card(
        "blue",
        "[linear] Test notification".into(),
        vec![md_div("If you can read this, routing delivery works.")],
    );
    let res = match p.kind.as_str() {
        "chat" => bot.reply_to_chat(&p.target, &test).await,
        "dm" => bot.send_dm(&p.target, &test).await,
        other => bail!("unknown kind '{other}' (expected 'chat' or 'dm')"),
    };
    res.map_err(|e| anyhow!("{e}"))?;
    Ok(format!("sent test to {} {}", p.kind, p.target))
}
