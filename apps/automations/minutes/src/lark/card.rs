//! Digest card — shown to the meeting owner (and optionally the fallback chat)
//! with a link to the full transcript doc.

use larkoapi::LarkCard;
use serde_json::json;

use crate::stt::Transcript;

pub struct DigestCardInput<'a> {
    pub topic: &'a str,
    pub duration_ms: u64,
    pub doc_url: &'a str,
    pub recording_url: Option<&'a str>,
    pub transcript: &'a Transcript,
}

pub fn build_digest_card(input: DigestCardInput<'_>) -> LarkCard {
    let title = format!("📝 Meeting Digest · {}", input.topic);
    let body = build_body(&input);

    let mut actions = vec![json!({
        "tag": "button",
        "text": {"tag": "plain_text", "content": "Open Transcript"},
        "type": "primary",
        "url": input.doc_url,
        "multi_url": {
            "url": input.doc_url,
            "android_url": input.doc_url,
            "ios_url": input.doc_url,
            "pc_url": input.doc_url,
        }
    })];
    if let Some(rec) = input.recording_url {
        actions.push(json!({
            "tag": "button",
            "text": {"tag": "plain_text", "content": "Recording"},
            "type": "default",
            "url": rec,
            "multi_url": {
                "url": rec,
                "android_url": rec,
                "ios_url": rec,
                "pc_url": rec,
            }
        }));
    }

    LarkCard::new("blue", title)
        .push(json!({
            "tag": "div",
            "text": {"tag": "lark_md", "content": body}
        }))
        .push(json!({
            "tag": "action",
            "actions": actions
        }))
}

fn build_body(input: &DigestCardInput<'_>) -> String {
    let mut lines = Vec::new();
    if input.duration_ms > 0 {
        lines.push(format!("**Duration:** {}", fmt_duration(input.duration_ms)));
    }
    if let Some(lang) = &input.transcript.language {
        lines.push(format!("**Language:** {lang}"));
    }
    lines.push(format!("**Segments:** {}", input.transcript.segments.len()));
    if let Some(preview) = preview(&input.transcript.full_text, 240) {
        lines.push(format!("\n**Preview:**\n{preview}"));
    }
    lines.join("\n")
}

fn preview(text: &str, max_chars: usize) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::new();
    for ch in trimmed.chars().take(max_chars) {
        out.push(ch);
    }
    if trimmed.chars().count() > max_chars {
        out.push_str(" …");
    }
    Some(out)
}

fn fmt_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}
