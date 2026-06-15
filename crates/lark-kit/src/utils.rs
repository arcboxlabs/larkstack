//! Shared helpers: HMAC verification and text truncation.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Verifies an HMAC-SHA256 hex signature over `body` keyed by `secret`. Shared
/// by every webhook source (Linear `linear-signature`, GitHub
/// `X-Hub-Signature-256`, …).
pub fn verify_hmac_sha256(secret: &str, body: &[u8], hex_sig: &str) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    expected == hex_sig
}

/// Verifies a [Standard Webhooks] signature, the scheme GitLab 19.1+ uses for
/// its webhook *signing token*.
///
/// `secret` is the configured signing token (`whsec_<base64>`, prefix optional);
/// `id` / `timestamp` are the `webhook-id` / `webhook-timestamp` header values;
/// and `sig_header` is the raw `webhook-signature` value — a space-separated
/// list of `v1,<base64>` entries. The signature is HMAC-SHA256 over
/// `"{id}.{timestamp}.{body}"`, keyed by the base64-decoded secret; the request
/// is accepted if any list entry matches. `timestamp` is also required to be
/// within `tolerance_secs` of `now_unix` (replay defense), so callers pass the
/// current Unix time and a tolerance (e.g. 300s).
///
/// [Standard Webhooks]: https://www.standardwebhooks.com/
pub fn verify_standard_webhook(
    secret: &str,
    id: &str,
    timestamp: &str,
    body: &[u8],
    sig_header: &str,
    now_unix: i64,
    tolerance_secs: i64,
) -> bool {
    let Ok(ts) = timestamp.parse::<i64>() else {
        return false;
    };
    if (now_unix - ts).abs() > tolerance_secs {
        return false;
    }

    // The HMAC key is the secret with the `whsec_` prefix stripped, base64-decoded.
    let raw = secret.strip_prefix("whsec_").unwrap_or(secret);
    let Ok(key) = B64.decode(raw) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&key) else {
        return false;
    };
    mac.update(id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    let expected = B64.encode(mac.finalize().into_bytes());

    sig_header
        .split(' ')
        .filter_map(|entry| entry.strip_prefix("v1,"))
        .any(|sig| sig == expected)
}

/// Truncates `s` to at most `max_chars` characters, appending `"…"` when
/// truncation occurs.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::{B64, truncate, verify_standard_webhook};
    use base64::Engine as _;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    #[test]
    fn truncate_appends_ellipsis_only_when_needed() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "hel…");
        // Counts characters, not bytes.
        assert_eq!(truncate("héllo", 4), "héll…");
    }

    /// Signs like a Standard Webhooks sender: `whsec_<b64key>` secret, signature
    /// = base64(HMAC-SHA256(key, "{id}.{ts}.{body}")), header `v1,<sig>`.
    fn sign(secret: &str, id: &str, ts: &str, body: &[u8]) -> String {
        let raw = secret.strip_prefix("whsec_").unwrap_or(secret);
        let key = B64.decode(raw).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        mac.update(format!("{id}.{ts}.").as_bytes());
        mac.update(body);
        format!("v1,{}", B64.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn accepts_valid_signature_within_window() {
        let secret = format!("whsec_{}", B64.encode(b"super-secret-key"));
        let (id, ts, body) = ("msg_1", "1000", br#"{"object_kind":"push"}"#.as_slice());
        let header = sign(&secret, id, ts, body);
        // Pick any later entry too — accept if any matches.
        let multi = format!("v1,bogus {header}");
        assert!(verify_standard_webhook(
            &secret, id, ts, body, &multi, 1100, 300
        ));
    }

    #[test]
    fn rejects_wrong_secret_tampered_body_and_stale_timestamp() {
        let secret = format!("whsec_{}", B64.encode(b"super-secret-key"));
        let (id, ts, body) = ("msg_1", "1000", br#"{"object_kind":"push"}"#.as_slice());
        let header = sign(&secret, id, ts, body);

        // Wrong secret.
        let other = format!("whsec_{}", B64.encode(b"different-key"));
        assert!(!verify_standard_webhook(
            &other, id, ts, body, &header, 1100, 300
        ));
        // Tampered body.
        assert!(!verify_standard_webhook(
            &secret,
            id,
            ts,
            b"tampered",
            &header,
            1100,
            300
        ));
        // Stale timestamp (outside tolerance).
        assert!(!verify_standard_webhook(
            &secret, id, ts, body, &header, 9999, 300
        ));
        // Non-numeric timestamp.
        assert!(!verify_standard_webhook(
            &secret, id, "nope", body, &header, 1100, 300
        ));
    }
}
