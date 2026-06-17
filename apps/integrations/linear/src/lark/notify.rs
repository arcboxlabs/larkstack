//! Sends Linear DM cards to Lark via the bot. Group-chat cards go through
//! [`lark_kit::routing`] instead (see [`crate::routes::webhook`]).

use lark_kit::card::LarkCard;
use tracing::error;

use crate::config::AppState;

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
