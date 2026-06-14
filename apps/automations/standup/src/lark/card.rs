//! Announce + reminder card builders. The announce card's wording adapts to
//! how far off `date` is (today / tomorrow / further out).

use chrono::NaiveDate;
use larkoapi::LarkCard;
use serde_json::{Value, json};

use crate::date;

pub fn build_announce_card(doc_url: &str, date: NaiveDate) -> LarkCard {
    let (title_prefix, body) = match (date - date::today()).num_days() {
        0 => (
            "今日",
            "Standup 文档已就位,如未填写请立即补上。".to_string(),
        ),
        1 => (
            "明日",
            "Standup 文档已就位。请在 **明早 10:30 之前** 完成填写。".to_string(),
        ),
        n if n > 0 => ("", format!("Standup 文档已就位 ({n} 天后),请按时填写。")),
        _ => ("", "Standup 文档链接见下。".to_string()),
    };
    let title = if title_prefix.is_empty() {
        format!("Daily Standup · {}", date.format("%Y-%m-%d"))
    } else {
        format!("{title_prefix} Daily Standup · {}", date.format("%Y-%m-%d"))
    };
    LarkCard::new("blue", title)
        .push(json!({
            "tag": "div",
            "text": {"tag": "lark_md", "content": body}
        }))
        .push(open_doc_action(doc_url))
}

pub fn build_reminder_card(doc_url: &str, urgent: bool) -> LarkCard {
    let (template, title, body) = if urgent {
        (
            "red",
            "⚠️ Daily Standup 最后提醒",
            "Standup 马上开始,请立刻填写你的那一行。",
        )
    } else {
        (
            "orange",
            "📝 Daily Standup 提醒",
            "你还没填写今天的 Daily Standup,请尽快完成。",
        )
    };
    LarkCard::new(template, title)
        .push(json!({
            "tag": "div",
            "text": {"tag": "lark_md", "content": body}
        }))
        .push(open_doc_action(doc_url))
}

fn open_doc_action(doc_url: &str) -> Value {
    json!({
        "tag": "action",
        "actions": [{
            "tag": "button",
            "text": {"tag": "plain_text", "content": "打开文档"},
            "type": "primary",
            "url": doc_url,
            "multi_url": {
                "url": doc_url,
                "android_url": doc_url,
                "ios_url": doc_url,
                "pc_url": doc_url
            }
        }]
    })
}
