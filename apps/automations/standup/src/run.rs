use std::sync::Arc;

use larkoapi::{LarkBotClient, WsEventHandler, ws};
use larkstack_core::ControlHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::commands::CommandBot;
use crate::config::AppConfig;
use crate::scheduler;

/// Build the Lark bot client from a typed config. Synchronous and cheap.
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

/// Spawn the WS command handler (best-effort) and run the scheduler until
/// `cancel` fires. Shared by the embedded App instance and the standalone
/// binary. The WS task is tied to `cancel` so a restart doesn't leak it.
pub async fn serve_with_bot(
    cfg: &AppConfig,
    bot: Arc<LarkBotClient>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
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
            let ws_cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = ws::run_ws_client(&base_url, &app_id, &app_secret, handler, http_ws) => {}
                    _ = ws_cancel.cancelled() => {}
                }
            });
        }
        Err(e) => warn!("standup: bot_open_id lookup failed ({e}); command bot disabled"),
    }

    tokio::select! {
        _ = scheduler::run(cfg.standup.clone(), bot) => {}
        _ = cancel.cancelled() => info!("standup: scheduler stopped"),
    }
    Ok(())
}

/// Standalone runner with its own [`ControlHandle`].
pub async fn run_with_bot(cfg: AppConfig, bot: Arc<LarkBotClient>, handle: ControlHandle) {
    handle.running().await;
    let _ = serve_with_bot(&cfg, bot, CancellationToken::new()).await;
}

/// Convenience: build + run, for the standalone binary.
pub async fn run(cfg: AppConfig, handle: ControlHandle) -> anyhow::Result<()> {
    let bot = build_bot(&cfg)?;
    run_with_bot(cfg, bot, handle).await;
    Ok(())
}
