//! GitHub-specific helpers: webhook signature verification.

/// Verifies the `X-Hub-Signature-256` header.
///
/// GitHub sends the signature as `sha256=<hex>`. This strips the prefix and
/// delegates to the shared HMAC verifier.
pub fn verify_github_signature(secret: &str, body: &[u8], header_value: &str) -> bool {
    let Some(hex_sig) = header_value.strip_prefix("sha256=") else {
        return false;
    };
    crate::utils::verify_hmac_sha256(secret, body, hex_sig)
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    use super::verify_github_signature;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn accepts_valid_prefixed_signature() {
        let body = br#"{"hello":"world"}"#;
        let header = format!("sha256={}", sign("s3cret", body));
        assert!(verify_github_signature("s3cret", body, &header));
    }

    #[test]
    fn rejects_signature_without_sha256_prefix() {
        let body = b"payload";
        // Correct hex, but GitHub always prefixes `sha256=` — a bare hex is rejected.
        let bare = sign("s3cret", body);
        assert!(!verify_github_signature("s3cret", body, &bare));
    }

    #[test]
    fn rejects_tampered_signature_and_secret() {
        let body = b"payload";
        assert!(!verify_github_signature("s3cret", body, "sha256=deadbeef"));
        let header = format!("sha256={}", sign("right-secret", body));
        assert!(!verify_github_signature("wrong-secret", body, &header));
    }
}
