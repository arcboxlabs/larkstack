use tracing::warn;

use super::model::{DestKind, Destination};
use crate::LarkBotClient;
use crate::card::LarkCard;

/// Deliver `card` to a single [`Destination`] via the bot. Logs and skips when no bot is
/// configured or the send fails (delivery is best-effort, like the group-webhook sender).
pub async fn deliver(bot: Option<&LarkBotClient>, dest: &Destination, card: &LarkCard) {
    let Some(bot) = bot else {
        warn!(
            "routing: no Lark bot configured — cannot deliver to {:?} {}",
            dest.kind, dest.target
        );
        return;
    };
    let res = match dest.kind {
        DestKind::Chat => bot.reply_to_chat(&dest.target, card).await,
        // DM targets are user `open_id`s (from the console picker); a target that looks
        // like an email (`@`) is still delivered by email for back-compat / manual entry.
        DestKind::Dm if dest.target.contains('@') => bot.send_dm(&dest.target, card).await,
        DestKind::Dm => bot.send_dm_by_open_id(&dest.target, card).await,
    };
    if let Err(e) = res {
        warn!(
            "routing: failed to deliver to {:?} {}: {e}",
            dest.kind, dest.target
        );
    }
}

/// Deliver `card` to every destination in turn.
pub async fn deliver_all(bot: Option<&LarkBotClient>, dests: &[Destination], card: &LarkCard) {
    for d in dests {
        deliver(bot, d, card).await;
    }
}
