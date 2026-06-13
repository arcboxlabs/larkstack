//! Lark (Feishu) notification sink — group webhook cards and bot DMs.

mod bot;
pub mod cards;
pub mod event_handler;
pub mod models;
mod webhook;

pub use bot::LarkBotClient;
pub use event_handler::lark_event_handler;

use reqwest::Client;
use tracing::error;

use crate::event::Event;

/// Sends a card notification for `event` to the given Lark group webhook.
pub async fn notify(event: &Event, http: &Client, webhook_url: &str) {
    let card = cards::build_lark_card(event);
    webhook::send_lark_card(http, webhook_url, &card).await;
}

/// DMs the recipient about `event` (assignment or review request). No-op for
/// event types that don't warrant a DM.
pub async fn try_dm(event: &Event, bot: &LarkBotClient, email: &str) {
    let Some(card) = cards::build_assign_dm_card(event) else {
        return;
    };
    if let Err(e) = bot.send_dm(email, &card).await {
        error!("failed to DM {email}: {e}");
    }
}
