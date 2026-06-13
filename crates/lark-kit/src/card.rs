//! Lark interactive-card building blocks, shared by every integration's cards.

use serde_json::{Value, json};

pub use larkoapi::card::{LarkCard, LarkHeader, LarkMessage, LarkTitle};

/// A `lark_md` text `div` element.
pub fn md_div(content: &str) -> Value {
    json!({
        "tag": "div",
        "text": { "tag": "lark_md", "content": content },
    })
}

/// A single-button action row linking to `url` with a custom label.
pub fn link_button(url: &str, label: &str) -> Value {
    json!({
        "tag": "action",
        "actions": [{
            "tag": "button",
            "text": { "tag": "plain_text", "content": label },
            "type": "primary",
            "url": url,
        }]
    })
}

/// Builds a [`LarkCard`] with a colored header and the given elements. Use for
/// DM and link-preview cards; wrap with [`message`] for group-webhook delivery.
pub fn card(color: &str, header: String, elements: Vec<Value>) -> LarkCard {
    LarkCard {
        config: None,
        header: LarkHeader {
            template: color.to_string(),
            title: LarkTitle {
                content: header,
                tag: "plain_text",
            },
        },
        elements,
    }
}

/// Wraps a [`LarkCard`] in an interactive [`LarkMessage`] for webhook delivery.
pub fn message(card: LarkCard) -> LarkMessage {
    LarkMessage {
        msg_type: "interactive",
        card,
    }
}
