//! Axum handler for `POST /lark/event` — X link previews via Lark's
//! `url.preview.get` callback. Decryption/token/challenge handling lives in
//! `lark_kit::event`; this only fetches the tweet and builds the card.

use std::sync::Arc;

use axum::{Json, body::Bytes, extract::State, http::StatusCode};
use lark_kit::event::{Callback, classify, inline_card};
use serde_json::{Value, json};
use tracing::info;

use crate::config::AppState;

pub async fn lark_event_handler(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    match classify(
        &body,
        state.lark.verification_token.as_deref(),
        state.lark.encrypt_key.as_deref(),
    ) {
        Callback::BadRequest => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid json" })),
        ),
        Callback::Challenge(challenge) => (StatusCode::OK, Json(json!({ "challenge": challenge }))),
        Callback::Unauthorized => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        ),
        Callback::Ignored => (StatusCode::OK, Json(json!({}))),
        Callback::Preview { url } => {
            let Some(tweet_id) = crate::source::extract_tweet_id(&url) else {
                info!("no tweet id in preview url: {url}");
                return (StatusCode::OK, Json(json!({})));
            };
            info!("fetching tweet {tweet_id} for link preview");
            let tweet = state.x.fetch(tweet_id, &url).await;
            let (card, title) = crate::cards::x_preview(&tweet);
            (StatusCode::OK, Json(inline_card(&title, card)))
        }
    }
}
