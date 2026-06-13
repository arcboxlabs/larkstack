//! Lark event-callback (`POST /lark/event`) scaffolding shared by the
//! link-preview apps: AES-256-CBC decryption, challenge handshake, token check,
//! and `url.preview.get` classification. Each app only supplies the URL→card
//! fetch for the [`Callback::Preview`] case.

use serde_json::Value;

/// The classified outcome of a Lark event callback.
pub enum Callback {
    /// Body was not valid JSON — respond `400`.
    BadRequest,
    /// `url_verification` handshake — echo this challenge back.
    Challenge(String),
    /// Decryption or token verification failed — respond `401`.
    Unauthorized,
    /// A `url.preview.get` for this URL — the app fetches + builds a card.
    Preview { url: String },
    /// Any other event — respond `200 {}`.
    Ignored,
}

/// Decrypts (if needed), validates, and classifies a raw callback body.
///
/// `verification_token` / `encrypt_key` come from the app's [`LarkConfig`](crate::LarkConfig);
/// `None` for either disables that check.
pub fn classify(
    body: &[u8],
    verification_token: Option<&str>,
    encrypt_key: Option<&str>,
) -> Callback {
    let Ok(raw) = serde_json::from_slice::<Value>(body) else {
        return Callback::BadRequest;
    };

    // Decrypt when Lark sent an encrypted envelope.
    let value = match raw.get("encrypt").and_then(Value::as_str) {
        Some(encrypted) => match encrypt_key.and_then(|k| decrypt(k, encrypted)) {
            Some(v) => v,
            None => return Callback::Unauthorized,
        },
        None => raw,
    };

    if value.get("type").and_then(Value::as_str) == Some("url_verification") {
        let challenge = value
            .get("challenge")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Callback::Challenge(challenge);
    }

    if let Some(expected) = verification_token {
        let incoming = value
            .get("header")
            .and_then(|h| h.get("token"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if incoming != expected {
            return Callback::Unauthorized;
        }
    }

    let event_type = value
        .get("header")
        .and_then(|h| h.get("event_type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if event_type == "url.preview.get" {
        return Callback::Preview {
            url: preview_url(&value).to_string(),
        };
    }

    Callback::Ignored
}

/// Wraps a card in the `url.preview.get` inline-response envelope.
pub fn inline_card(title: &str, card: impl serde::Serialize) -> Value {
    serde_json::json!({
        "inline": { "i18n_title": { "en_us": title, "zh_cn": title } },
        "card": { "type": "raw", "data": card }
    })
}

/// Extracts the URL from a `url.preview.get` event across the field shapes Lark
/// has used (`event.context.url`, `event.url`, `event.body.url`).
fn preview_url(value: &Value) -> &str {
    let event = value.get("event");
    event
        .and_then(|e| e.get("context"))
        .and_then(|c| c.get("url"))
        .and_then(Value::as_str)
        .or_else(|| event.and_then(|e| e.get("url")).and_then(Value::as_str))
        .or_else(|| {
            event
                .and_then(|e| e.get("body"))
                .and_then(|b| b.get("url"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
}

/// Decrypts a Lark AES-256-CBC envelope: key = SHA256(encrypt_key), IV = first
/// 16 decoded bytes, Pkcs7 padding. Returns `None` on any failure.
fn decrypt(encrypt_key: &str, encrypted: &str) -> Option<Value> {
    use aes::Aes256;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    use sha2::{Digest, Sha256};

    type Aes256CbcDec = cbc::Decryptor<Aes256>;

    let key = Sha256::digest(encrypt_key.as_bytes());
    let bytes = B64.decode(encrypted).ok()?;
    if bytes.len() < 16 {
        return None;
    }
    let (iv, ciphertext) = bytes.split_at(16);
    let decryptor = Aes256CbcDec::new_from_slices(&key, iv).ok()?;
    let mut buf = ciphertext.to_vec();
    let plaintext = decryptor.decrypt_padded_mut::<Pkcs7>(&mut buf).ok()?;
    serde_json::from_slice(plaintext).ok()
}

#[cfg(test)]
mod tests {
    use super::{Callback, classify, decrypt};

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
    fn decrypt_round_trips() {
        let enc = encrypt("k", br#"{"hello":"world"}"#);
        let v = decrypt("k", &enc).expect("decrypts");
        assert_eq!(v["hello"], "world");
    }

    #[test]
    fn decrypt_rejects_wrong_key_and_garbage() {
        let enc = encrypt("right", br#"{"a":1}"#);
        assert!(decrypt("wrong", &enc).is_none());
        assert!(decrypt("k", "not-base64-!!").is_none());
        assert!(decrypt("k", "c2hvcnQ=").is_none()); // < 16 bytes
    }

    #[test]
    fn classify_handles_challenge_and_token() {
        let challenge = br#"{"type":"url_verification","challenge":"xyz"}"#;
        assert!(matches!(
            classify(challenge, Some("tok"), None),
            Callback::Challenge(c) if c == "xyz"
        ));

        let event = br#"{"header":{"token":"tok","event_type":"url.preview.get"},"event":{"context":{"url":"https://x.com/a/status/1"}}}"#;
        assert!(matches!(
            classify(event, Some("tok"), None),
            Callback::Preview { url } if url == "https://x.com/a/status/1"
        ));
        assert!(matches!(
            classify(event, Some("nope"), None),
            Callback::Unauthorized
        ));
    }

    #[test]
    fn classify_decrypts_then_classifies() {
        let plain = br#"{"header":{"event_type":"url.preview.get"},"event":{"url":"https://linear.app/x/issue/AB-1/y"}}"#;
        let enc = encrypt("ek", plain);
        let body = format!(r#"{{"encrypt":"{enc}"}}"#);
        assert!(matches!(
            classify(body.as_bytes(), None, Some("ek")),
            Callback::Preview { url } if url.contains("AB-1")
        ));
        // Encrypted body but no key configured → unauthorized.
        assert!(matches!(
            classify(body.as_bytes(), None, None),
            Callback::Unauthorized
        ));
    }
}
