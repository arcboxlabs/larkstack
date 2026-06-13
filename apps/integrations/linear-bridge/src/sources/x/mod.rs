//! X (Twitter) source — fetches tweet data for Lark link previews.
//!
//! Fetch priority:
//! 1. fxtwitter (`api.fxtwitter.com`) — no auth, rich data including metrics
//! 2. X API v2 — requires `X_BEARER_TOKEN`, adds little over fxtwitter
//! 3. oEmbed — no auth, minimal data (author name + HTML-extracted text)

use reqwest::Client;
use tracing::warn;

/// Minimal tweet data needed to build a Lark preview card.
pub struct TweetData {
    pub text: String,
    pub author_name: String,
    pub author_username: String,
    pub url: String,
    pub like_count: Option<u64>,
    pub retweet_count: Option<u64>,
}

/// Extracts `(username, tweet_id)` from an X or Twitter URL.
///
/// Handles `https://x.com/user/status/1234567890` and
/// `https://twitter.com/user/status/1234567890`.
pub fn extract_tweet_info(url: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = url.split('/').collect();
    for (i, &part) in parts.iter().enumerate() {
        if part == "status" {
            let id = parts.get(i + 1).copied()?;
            let id = id.split('?').next().unwrap_or(id);
            let id = id.split('#').next().unwrap_or(id);
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                let username = parts.get(i.wrapping_sub(1)).copied().unwrap_or("");
                return Some((username, id));
            }
        }
    }
    None
}

/// Extracts only the tweet ID from an X/Twitter URL.
pub fn extract_tweet_id(url: &str) -> Option<&str> {
    extract_tweet_info(url).map(|(_, id)| id)
}

/// Client for fetching tweet data.
pub struct XClient {
    bearer_token: Option<String>,
    http: Client,
}

impl XClient {
    pub fn new(bearer_token: Option<String>, http: Client) -> Self {
        Self { bearer_token, http }
    }

    /// Fetches tweet data, trying fxtwitter, then X API v2, then oEmbed.
    /// Always returns a [`TweetData`] (empty fields if every source fails).
    pub async fn fetch(&self, tweet_id: &str, tweet_url: &str) -> TweetData {
        // 1. fxtwitter — best option, no auth needed.
        let username = extract_tweet_info(tweet_url).map(|(u, _)| u).unwrap_or("");
        if !username.is_empty() {
            match self.fetch_fxtwitter(username, tweet_id).await {
                Ok(data) => return data,
                Err(e) => warn!("fxtwitter failed, trying next source: {e}"),
            }
        }

        // 2. X API v2 (optional bearer token).
        if let Some(token) = &self.bearer_token {
            match self.fetch_api_v2(tweet_id, token).await {
                Ok(data) => return data,
                Err(e) => warn!("X API v2 failed, falling back to oEmbed: {e}"),
            }
        }

        // 3. oEmbed fallback.
        match self.fetch_oembed(tweet_url).await {
            Ok(data) => data,
            Err(e) => {
                warn!("X oEmbed also failed: {e}");
                TweetData {
                    text: String::new(),
                    author_name: String::new(),
                    author_username: String::new(),
                    url: tweet_url.to_string(),
                    like_count: None,
                    retweet_count: None,
                }
            }
        }
    }

    async fn fetch_fxtwitter(&self, username: &str, tweet_id: &str) -> Result<TweetData, String> {
        let url = format!("https://api.fxtwitter.com/{username}/status/{tweet_id}");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("request error: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("fxtwitter returned {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

        let tweet = &body["tweet"];
        let author = &tweet["author"];

        Ok(TweetData {
            text: tweet["text"].as_str().unwrap_or("").to_string(),
            author_name: author["name"].as_str().unwrap_or("").to_string(),
            author_username: author["screen_name"].as_str().unwrap_or("").to_string(),
            url: format!("https://x.com/{username}/status/{tweet_id}"),
            like_count: tweet["likes"].as_u64(),
            retweet_count: tweet["retweets"].as_u64(),
        })
    }

    async fn fetch_api_v2(&self, tweet_id: &str, bearer_token: &str) -> Result<TweetData, String> {
        let url = format!(
            "https://api.twitter.com/2/tweets/{tweet_id}?expansions=author_id&user.fields=name,username&tweet.fields=public_metrics"
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|e| format!("request error: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("X API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

        let user = &body["includes"]["users"][0];
        let metrics = &body["data"]["public_metrics"];

        Ok(TweetData {
            text: body["data"]["text"].as_str().unwrap_or("").to_string(),
            author_name: user["name"].as_str().unwrap_or("").to_string(),
            author_username: user["username"].as_str().unwrap_or("").to_string(),
            url: format!("https://x.com/i/web/status/{tweet_id}"),
            like_count: metrics["like_count"].as_u64(),
            retweet_count: metrics["retweet_count"].as_u64(),
        })
    }

    async fn fetch_oembed(&self, tweet_url: &str) -> Result<TweetData, String> {
        let url = format!("https://publish.twitter.com/oembed?url={tweet_url}&omit_script=true");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("request error: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("oEmbed returned {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

        Ok(TweetData {
            text: body["html"]
                .as_str()
                .map(extract_oembed_text)
                .unwrap_or_default(),
            author_name: body["author_name"].as_str().unwrap_or("").to_string(),
            author_username: String::new(),
            url: tweet_url.to_string(),
            like_count: None,
            retweet_count: None,
        })
    }
}

/// Pulls the tweet text out of oEmbed's embedded `<p>…</p>` HTML, stripping tags.
fn extract_oembed_text(html: &str) -> String {
    let Some(start) = html.find("<p") else {
        return String::new();
    };
    let Some(open_end) = html[start..].find('>').map(|i| i + start + 1) else {
        return String::new();
    };
    let Some(close) = html[open_end..].find("</p>").map(|i| i + open_end) else {
        return String::new();
    };

    let mut out = String::with_capacity(close - open_end);
    let mut in_tag = false;
    for ch in html[open_end..close].chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{extract_oembed_text, extract_tweet_info};

    #[test]
    fn parses_x_and_twitter_status_urls() {
        assert_eq!(
            extract_tweet_info("https://x.com/jack/status/123"),
            Some(("jack", "123"))
        );
        assert_eq!(
            extract_tweet_info("https://twitter.com/Bob/status/456"),
            Some(("Bob", "456"))
        );
    }

    #[test]
    fn strips_query_and_fragment_from_id() {
        assert_eq!(
            extract_tweet_info("https://x.com/u/status/789?s=20&t=x"),
            Some(("u", "789"))
        );
        assert_eq!(
            extract_tweet_info("https://x.com/u/status/789#anchor"),
            Some(("u", "789"))
        );
    }

    #[test]
    fn rejects_non_status_or_non_numeric_urls() {
        assert_eq!(extract_tweet_info("https://x.com/jack"), None);
        assert_eq!(extract_tweet_info("https://x.com/u/status/notanid"), None);
        assert_eq!(extract_tweet_info("https://github.com/a/b"), None);
    }

    #[test]
    fn oembed_text_strips_nested_tags() {
        let html =
            r#"<blockquote><p lang="en">Hello <a href="x">world</a>!</p>&mdash; me</blockquote>"#;
        assert_eq!(extract_oembed_text(html), "Hello world!");
    }
}
