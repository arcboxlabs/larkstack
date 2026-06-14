//! Sends Linear notification cards to Lark — group webhook + assignee DM.

use lark_kit::card::{LarkCard, LarkMessage};
use tracing::error;

use crate::config::AppState;

/// Posts a card to the configured Lark group webhook.
pub async fn group(state: &AppState, card: &LarkMessage) {
    lark_kit::send_lark_card(&state.http, &state.lark.webhook_url, card).await;
}

/// DMs a single user by Lark email (no-op when no bot is configured).
pub async fn dm(state: &AppState, email: &str, card: &LarkCard) {
    if let Some(bot) = &state.bot
        && let Err(e) = bot.send_dm(email, card).await
    {
        error!("failed to DM {email}: {e}");
    }
}

/// DMs the same card to several users by Lark email (best-effort; per-recipient
/// failures are logged and skipped).
pub async fn dm_many(state: &AppState, emails: &[String], card: &LarkCard) {
    for email in emails {
        dm(state, email, card).await;
    }
}
