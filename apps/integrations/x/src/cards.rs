//! X (Twitter) link-preview card builder.

use lark_kit::card::{LarkCard, card, link_button, md_div};
use lark_kit::truncate;
use serde_json::json;

use crate::source::TweetData;

/// Builds an inline preview card from fetched tweet data. Returns
/// `(card, inline_title)` — the inline title is the short text shown in chat
/// before the card expands.
pub fn x_preview(tweet: &TweetData) -> (LarkCard, String) {
    let author_at = if tweet.author_username.is_empty() {
        tweet.author_name.clone()
    } else {
        format!("@{}", tweet.author_username)
    };

    let mut elements = vec![];
    if !tweet.text.is_empty() {
        elements.push(json!({
            "tag": "markdown",
            "content": truncate(&tweet.text, 200),
        }));
    }

    let note = if tweet.like_count.is_some() || tweet.retweet_count.is_some() {
        let likes = tweet.like_count.unwrap_or(0);
        let retweets = tweet.retweet_count.unwrap_or(0);
        format!("❤️ {likes}  🔁 {retweets}  •  {author_at} on X")
    } else if !author_at.is_empty() {
        format!("From {author_at} on X")
    } else {
        String::new()
    };
    if !note.is_empty() {
        elements.push(md_div(&note));
    }
    elements.push(link_button(&tweet.url, "View on X"));

    let header = if tweet.author_name.is_empty() {
        "X Post".to_string()
    } else {
        tweet.author_name.clone()
    };

    let inline_title = if !author_at.is_empty() && !tweet.text.is_empty() {
        format!("{}: {}...", author_at, truncate(&tweet.text, 30))
    } else if !author_at.is_empty() {
        format!("Post by {author_at}")
    } else {
        "X Post".to_string()
    };

    (card("blue", header, elements), inline_title)
}
