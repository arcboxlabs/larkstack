//! Announce + reminder card builders. Title/body wording comes from the
//! admin-editable [`Settings`] templates; only the card colors are fixed
//! (announce = blue, reminder = orange / red when urgent).

use chrono::NaiveDate;
use larkoapi::LarkCard;
use minijinja::context;
use serde_json::{Value, json};

use crate::settings::Settings;
use crate::{date, template};

pub fn build_announce_card(settings: &Settings, doc_url: &str, date: NaiveDate) -> LarkCard {
    let days_until = (date - date::today(settings.timezone)).num_days();
    let date_str = date.format("%Y-%m-%d").to_string();
    let title = template::render(
        &settings.announce_title,
        context! { date => date_str, days_until },
    );
    let body = template::render(
        &settings.announce_body,
        context! { date => date_str, days_until, url => doc_url },
    );
    LarkCard::new("blue", title.trim())
        .push(json!({
            "tag": "div",
            "text": {"tag": "lark_md", "content": body}
        }))
        .push(open_doc_action(doc_url))
}

pub fn build_reminder_card(settings: &Settings, doc_url: &str, urgent: bool) -> LarkCard {
    let color = if urgent { "red" } else { "orange" };
    let title = template::render(&settings.reminder_title, context! { urgent });
    let body = template::render(&settings.reminder_body, context! { urgent, url => doc_url });
    LarkCard::new(color, title.trim())
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
