//! GitLab webhook authentication.
//!
//! GitLab offers two mechanisms; we accept either. The modern GitLab 19.1+
//! [Standard Webhooks] *signing token* (`webhook-signature` header) is preferred
//! when a `signing_secret` is configured and the header is present; otherwise we
//! fall back to the legacy `X-Gitlab-Token` plaintext shared secret.
//!
//! [Standard Webhooks]: https://www.standardwebhooks.com/

use axum::http::HeaderMap;

use crate::config::GitLabConfig;

/// How far a Standard Webhooks `webhook-timestamp` may deviate from now, in
/// seconds, before the request is rejected as a possible replay.
const SIGNING_TOLERANCE_SECS: i64 = 300;

/// Authenticates an inbound webhook request against the configured secrets.
/// Returns `false` when no configured method authenticates the request.
pub fn authenticate(headers: &HeaderMap, body: &[u8], cfg: &GitLabConfig) -> bool {
    // Prefer the signing token when both the secret and the header are present;
    // its result is then authoritative (no fallback to the weaker token).
    if let (Some(secret), Some(sig)) = (
        cfg.signing_secret.as_deref(),
        header(headers, "webhook-signature"),
    ) {
        let id = header(headers, "webhook-id").unwrap_or_default();
        let ts = header(headers, "webhook-timestamp").unwrap_or_default();
        return lark_kit::verify_standard_webhook(
            secret,
            id,
            ts,
            body,
            sig,
            now_unix(),
            SIGNING_TOLERANCE_SECS,
        );
    }
    match header(headers, "x-gitlab-token") {
        Some(token) => verify_token(&cfg.webhook_token, token),
        None => false,
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Legacy `X-Gitlab-Token`: a PLAINTEXT shared secret, *not* an HMAC over the
/// body. Plain `==` matches the house style (`lark_kit::verify_hmac_sha256` also
/// ends in `expected == sig`; no constant-time dependency exists in the workspace).
fn verify_token(expected: &str, header_value: &str) -> bool {
    !expected.is_empty() && expected == header_value
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{authenticate, verify_token};
    use crate::config::GitLabConfig;
    use axum::http::HeaderMap;

    fn cfg(token: &str, signing: Option<&str>) -> GitLabConfig {
        GitLabConfig {
            webhook_token: token.into(),
            signing_secret: signing.map(Into::into),
        }
    }

    #[test]
    fn verify_token_matches_exactly() {
        assert!(verify_token("s3cret", "s3cret"));
        assert!(!verify_token("s3cret", "wrong"));
        // An empty configured token never authenticates (even against empty input).
        assert!(!verify_token("", ""));
    }

    #[test]
    fn authenticates_via_gitlab_token() {
        let mut h = HeaderMap::new();
        h.insert("x-gitlab-token", "s3cret".parse().unwrap());
        assert!(authenticate(&h, b"{}", &cfg("s3cret", None)));

        let mut bad = HeaderMap::new();
        bad.insert("x-gitlab-token", "nope".parse().unwrap());
        assert!(!authenticate(&bad, b"{}", &cfg("s3cret", None)));
    }

    #[test]
    fn missing_auth_header_is_rejected() {
        assert!(!authenticate(
            &HeaderMap::new(),
            b"{}",
            &cfg("s3cret", None)
        ));
    }

    #[test]
    fn signing_header_is_authoritative_over_token() {
        // Signing secret configured + signing header present → the signing path is
        // used. A stale timestamp makes it fail; we must NOT fall back to the
        // (otherwise valid) X-Gitlab-Token also present on the request.
        let mut h = HeaderMap::new();
        h.insert("x-gitlab-token", "s3cret".parse().unwrap());
        h.insert("webhook-signature", "v1,bogus".parse().unwrap());
        h.insert("webhook-id", "msg_1".parse().unwrap());
        h.insert("webhook-timestamp", "0".parse().unwrap());
        let secret = "whsec_dGVzdGtleQ=="; // base64("testkey")
        assert!(!authenticate(&h, b"{}", &cfg("s3cret", Some(secret))));
    }

    #[test]
    fn falls_back_to_token_when_no_signature_header() {
        // Signing secret configured but the request only carries X-Gitlab-Token
        // (e.g. a webhook still on the legacy scheme) → token path is used.
        let mut h = HeaderMap::new();
        h.insert("x-gitlab-token", "s3cret".parse().unwrap());
        assert!(authenticate(
            &h,
            b"{}",
            &cfg("s3cret", Some("whsec_dGVzdGtleQ=="))
        ));
    }
}
