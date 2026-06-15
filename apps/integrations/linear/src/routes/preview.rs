//! Axum handler for `POST /lark/event` — Linear issue link previews via Lark's
//! `url.preview.get` callback. Decryption/token/challenge handling lives in
//! `lark_kit::event`.

use axum::{Json, body::Bytes, http::StatusCode};
use lark_kit::Live;
use lark_kit::event::{Callback, classify, inline_card};
use serde_json::{Value, json};
use tracing::{error, info};

use crate::config::AppState;
use crate::lark::cards;
use crate::source::api::extract_identifier_from_url;

pub async fn lark_event_handler(
    Live(state): Live<AppState>,
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
            let Some(linear) = &state.linear_client else {
                info!("link preview requested but LINEAR_API_KEY not configured");
                return (StatusCode::OK, Json(json!({})));
            };
            let Some(identifier) = extract_identifier_from_url(&url) else {
                info!("could not extract Linear identifier from URL: {url}");
                return (StatusCode::OK, Json(json!({})));
            };
            info!("fetching Linear issue {identifier} for link preview");
            match linear.fetch_issue_by_identifier(&identifier).await {
                Ok(issue) => {
                    let title = format!("[{}] {}", issue.identifier, issue.title);
                    (
                        StatusCode::OK,
                        Json(inline_card(&title, cards::preview_card(&issue))),
                    )
                }
                Err(e) => {
                    error!("failed to fetch Linear issue {identifier}: {e}");
                    (
                        StatusCode::OK,
                        Json(json!({ "inline": { "i18n_title": { "en_us": identifier } } })),
                    )
                }
            }
        }
    }
}
