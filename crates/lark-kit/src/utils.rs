//! Shared helpers: HMAC verification and text truncation.

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
    use super::truncate;

    #[test]
    fn truncate_appends_ellipsis_only_when_needed() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "hel…");
        // Counts characters, not bytes.
        assert_eq!(truncate("héllo", 4), "héll…");
    }
}
