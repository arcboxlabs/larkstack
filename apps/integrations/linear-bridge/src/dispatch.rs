//! Routes an [`Event`] to every registered sink.

use crate::{config::AppState, event::Event, sinks};

/// Sends a Linear `event` to the Linear group webhook. If `dm_email` is
/// provided, a direct message is also sent to that address.
pub async fn dispatch(event: &Event, state: &AppState, dm_email: Option<&str>) {
    sinks::lark::notify(event, &state.http, &state.lark.webhook_url).await;

    if let (Some(email), Some(bot)) = (dm_email, &state.lark_bot) {
        sinks::lark::try_dm(event, bot, email).await;
    }
}

/// Sends a GitHub `event` to the GitHub group webhook (falling back to the
/// Linear webhook when `github_webhook_url` is unset). If `dm_email` is
/// provided, a direct message is also sent (e.g. a review request).
pub async fn dispatch_github(event: &Event, state: &AppState, dm_email: Option<&str>) {
    let url = if state.lark.github_webhook_url.is_empty() {
        &state.lark.webhook_url
    } else {
        &state.lark.github_webhook_url
    };
    sinks::lark::notify(event, &state.http, url).await;

    if let (Some(email), Some(bot)) = (dm_email, &state.lark_bot) {
        sinks::lark::try_dm(event, bot, email).await;
    }
}
