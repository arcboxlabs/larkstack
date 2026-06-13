use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Verifies an HMAC-SHA256 hex signature over `body` keyed by `secret`. Shared
/// by the Linear (`linear-signature`) and GitHub (`X-Hub-Signature-256`)
/// webhook sources.
pub fn verify_hmac_sha256(secret: &str, body: &[u8], hex_sig: &str) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    expected == hex_sig
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
