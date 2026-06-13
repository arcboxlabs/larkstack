//! Axum handler for `POST /lark/event` — Lark platform callbacks: challenge
//! verification, optional AES-256-CBC payload decryption, and `url.preview.get`
//! link unfurling for Linear issues and X (Twitter) posts.

use std::sync::Arc;

use axum::{Json, body::Bytes, extract::State, http::StatusCode};
use tracing::{error, info, warn};

use super::cards::{build_preview_card, build_x_preview_card};
use crate::{
    config::AppState,
    sources::{linear::client::extract_identifier_from_url, x::extract_tweet_id},
};

/// Decrypts a Lark AES-256-CBC encrypted callback.
///
/// Lark sends `{"encrypt": "<base64>"}` when an Encrypt Key is configured.
/// Key = SHA256(encrypt_key); IV = first 16 bytes of the decoded data.
fn decrypt_lark_payload(
    encrypt_key: &str,
    encrypted_data: &str,
) -> Result<serde_json::Value, String> {
    use aes::Aes256;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    use sha2::{Digest, Sha256};

    type Aes256CbcDec = cbc::Decryptor<Aes256>;

    let key = Sha256::digest(encrypt_key.as_bytes());
    let bytes = B64
        .decode(encrypted_data)
        .map_err(|e| format!("base64 decode: {e}"))?;
    if bytes.len() < 16 {
        return Err("encrypted payload too short".into());
    }
    let (iv, ciphertext) = bytes.split_at(16);

    let decryptor =
        Aes256CbcDec::new_from_slices(&key, iv).map_err(|e| format!("cipher init: {e}"))?;
    let mut buf = ciphertext.to_vec();
    let plaintext = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("decrypt: {e}"))?;

    serde_json::from_slice(plaintext).map_err(|e| format!("json parse: {e}"))
}

/// Handles incoming Lark event callbacks.
///
/// Supports `url_verification` challenges and `url.preview.get` link previews.
/// When an Encrypt Key (`x_encrypt_key`) is configured, AES-256-CBC encrypted
/// payloads are decrypted before processing. Callbacks may carry either the
/// Linear or X verification token (the two link-preview apps are distinct).
pub async fn lark_event_handler(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> (StatusCode, Json<serde_json::Value>) {
    let raw: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            error!("failed to parse lark event body: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid json"})),
            );
        }
    };

    // Decrypt if Lark sent an encrypted payload (Encrypt Key configured).
    let body_value = match raw.get("encrypt").and_then(|v| v.as_str()) {
        Some(encrypted) => match state.lark.x_encrypt_key.as_deref() {
            Some(key) => match decrypt_lark_payload(key, encrypted) {
                Ok(v) => v,
                Err(e) => {
                    error!("failed to decrypt lark payload: {e}");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({"error": "decryption failed"})),
                    );
                }
            },
            None => {
                warn!("received encrypted lark payload but x_encrypt_key is not set");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "encrypt key not configured"})),
                );
            }
        },
        None => raw,
    };

    if body_value.get("type").and_then(|v| v.as_str()) == Some("url_verification") {
        let challenge = body_value
            .get("challenge")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        info!("lark challenge verification");
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "challenge": challenge })),
        );
    }

    if !token_is_valid(&state, &body_value) {
        warn!("lark event token mismatch");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token"})),
        );
    }

    info!("lark event received");

    let event_type = body_value
        .get("header")
        .and_then(|h| h.get("event_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if event_type == "url.preview.get" {
        return handle_link_preview(&state, &body_value).await;
    }

    info!("ignoring lark event type: '{event_type}' – add handler if needed");
    (StatusCode::OK, Json(serde_json::json!({})))
}

/// Accepts the callback if no token is configured, or if it carries either the
/// Linear or the X verification token.
fn token_is_valid(state: &AppState, body: &serde_json::Value) -> bool {
    let configured = [
        state.lark.verification_token.as_deref(),
        state.lark.x_verification_token.as_deref(),
    ];
    if configured.iter().all(Option::is_none) {
        return true;
    }
    let incoming = body
        .get("header")
        .and_then(|h| h.get("token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    configured
        .into_iter()
        .flatten()
        .any(|expected| expected == incoming)
}

/// Handles `url.preview.get` — routes to X or Linear based on the URL.
async fn handle_link_preview(
    state: &AppState,
    body: &serde_json::Value,
) -> (StatusCode, Json<serde_json::Value>) {
    let event = body.get("event");
    let url = event
        .and_then(|e| e.get("context"))
        .and_then(|c| c.get("url"))
        .and_then(|v| v.as_str())
        .or_else(|| event.and_then(|e| e.get("url")).and_then(|v| v.as_str()))
        .or_else(|| {
            event
                .and_then(|e| e.get("body"))
                .and_then(|b| b.get("url"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("");

    // X / Twitter link.
    if let Some(tweet_id) = extract_tweet_id(url) {
        info!("fetching tweet {tweet_id} for link preview");
        let tweet = state.x_client.fetch(tweet_id, url).await;
        let (card, inline_title) = build_x_preview_card(&tweet);
        return (StatusCode::OK, Json(inline_card(&inline_title, card)));
    }

    // Linear link.
    let Some(ref linear) = state.linear_client else {
        info!("link preview requested but no handler matched URL: {url}");
        return (StatusCode::OK, Json(serde_json::json!({})));
    };
    let Some(identifier) = extract_identifier_from_url(url) else {
        info!("could not extract Linear identifier from URL: {url}");
        return (StatusCode::OK, Json(serde_json::json!({})));
    };

    info!("fetching Linear issue {identifier} for link preview");
    match linear.fetch_issue_by_identifier(&identifier).await {
        Ok(issue) => {
            let inline_title = format!("[{}] {}", issue.identifier, issue.title);
            let card = build_preview_card(&issue);
            (StatusCode::OK, Json(inline_card(&inline_title, card)))
        }
        Err(e) => {
            error!("failed to fetch Linear issue {identifier}: {e}");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "inline": { "i18n_title": { "en_us": identifier } }
                })),
            )
        }
    }
}

/// Wraps a card in the Lark `url.preview.get` inline-response envelope.
fn inline_card(title: &str, card: impl serde::Serialize) -> serde_json::Value {
    serde_json::json!({
        "inline": {
            "i18n_title": { "en_us": title, "zh_cn": title }
        },
        "card": { "type": "raw", "data": card }
    })
}

#[cfg(test)]
mod tests {
    use super::decrypt_lark_payload;

    /// Mirror of Lark's encryption: key = SHA256(encrypt_key), random-ish IV
    /// prepended, AES-256-CBC + Pkcs7.
    fn encrypt(encrypt_key: &str, plaintext: &[u8]) -> String {
        use aes::Aes256;
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD as B64;
        use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
        use sha2::{Digest, Sha256};

        type Aes256CbcEnc = cbc::Encryptor<Aes256>;

        let key = Sha256::digest(encrypt_key.as_bytes());
        let iv = [7u8; 16];
        let mut buf = vec![0u8; plaintext.len() + 16];
        let ct = Aes256CbcEnc::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_b2b_mut::<Pkcs7>(plaintext, &mut buf)
            .unwrap();
        let mut out = iv.to_vec();
        out.extend_from_slice(ct);
        B64.encode(out)
    }

    #[test]
    fn round_trips_an_encrypted_payload() {
        let payload = br#"{"type":"url_verification","challenge":"abc123"}"#;
        let enc = encrypt("my-encrypt-key", payload);
        let decrypted = decrypt_lark_payload("my-encrypt-key", &enc).expect("decrypts");
        assert_eq!(decrypted["type"], "url_verification");
        assert_eq!(decrypted["challenge"], "abc123");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let enc = encrypt("right-key", br#"{"a":1}"#);
        assert!(decrypt_lark_payload("wrong-key", &enc).is_err());
    }

    #[test]
    fn rejects_short_and_invalid_base64() {
        assert!(decrypt_lark_payload("k", "not base64 !!!").is_err());
        assert!(decrypt_lark_payload("k", "c2hvcnQ=").is_err()); // < 16 bytes decoded
    }
}
