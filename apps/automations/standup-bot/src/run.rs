use std::sync::Arc;

use larkstack_core::ControlHandle;
use larkoapi::{LarkBotClient, WsEventHandler, ws};
use tracing::{info, warn};

use crate::commands::CommandBot;
use crate::config::AppConfig;
use crate::scheduler;

/// Build the Lark bot client from a typed config. Synchronous, cheap, returns
/// immediately. Split out of [`run`] so callers (the console) can hand the
/// bot to other tasks (e.g. the action handler) before the scheduler/WS
/// loop blocks forever.
pub fn build_bot(cfg: &AppConfig) -> anyhow::Result<Arc<LarkBotClient>> {
    if cfg.lark.app_id.is_empty() || cfg.lark.app_secret.is_empty() {
        anyhow::bail!("LARK_APP_ID / LARK_APP_SECRET required");
    }
    let http = reqwest::Client::new();
    Ok(Arc::new(LarkBotClient::new(
        cfg.lark.app_id.clone(),
        cfg.lark.app_secret.clone(),
        cfg.lark.base_url.clone(),
        http,
    )))
}

/// Spawn the WS command handler (best-effort — disabled if `bot_open_id`
/// lookup fails) and run the scheduler in the foreground. Returns when the
/// scheduler exits.
pub async fn run_with_bot(cfg: AppConfig, bot: Arc<LarkBotClient>, handle: ControlHandle) {
    match bot.bot_open_id().await {
        Ok(bot_open_id) => {
            info!("standup: bot open_id = {bot_open_id}");
            let handler: Arc<dyn WsEventHandler> = Arc::new(CommandBot {
                cfg: Arc::new(cfg.standup.clone()),
                client: Arc::clone(&bot),
                bot_open_id,
            });
            let base_url = cfg.lark.base_url.clone();
            let app_id = cfg.lark.app_id.clone();
            let app_secret = cfg.lark.app_secret.clone();
            let http_ws = reqwest::Client::new();
            tokio::spawn(async move {
                ws::run_ws_client(&base_url, &app_id, &app_secret, handler, http_ws).await;
            });
        }
        Err(e) => warn!("standup: bot_open_id lookup failed ({e}); command bot disabled"),
    }
    handle.running().await;
    scheduler::run(cfg.standup, bot).await;
}

/// Convenience: build + run, for callers (standalone bin) that don't need
/// the bot handle.
pub async fn run(cfg: AppConfig, handle: ControlHandle) -> anyhow::Result<()> {
    let bot = build_bot(&cfg)?;
    run_with_bot(cfg, bot, handle).await;
    Ok(())
}
