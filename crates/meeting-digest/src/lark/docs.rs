//! Build a Lark Doc from a transcript, using `larkoapi`'s block helpers.
//!
//! Layout:
//!   H1  — meeting topic
//!   P   — metadata (duration, language, recording URL)
//!   H2  — "Transcript"
//!   P*  — one paragraph per segment, prefixed with `[HH:MM:SS]`
//!
//! Blocks are inserted in batches to stay under Lark's per-request limit.

use std::sync::Arc;

use larkoapi::LarkBotClient;
use serde_json::{Value, json};

use crate::stt::Transcript;

const BATCH_SIZE: usize = 40;

pub struct TranscriptDoc {
    pub doc_id: String,
    pub url: String,
}

pub struct TranscriptDocInput<'a> {
    pub folder_token: &'a str,
    pub title: String,
    pub topic: &'a str,
    pub duration_ms: u64,
    pub recording_url: Option<&'a str>,
    pub transcript: &'a Transcript,
}

pub async fn create_transcript_doc(
    client: &Arc<LarkBotClient>,
    input: TranscriptDocInput<'_>,
) -> Result<TranscriptDoc, String> {
    let doc_id = client
        .create_docx_in_folder(input.folder_token, &input.title)
        .await?;

    let header = heading_block(1, input.topic);
    let meta_text = build_meta_line(input.duration_ms, input.transcript, input.recording_url);
    let meta = paragraph_block(&meta_text);
    let section = heading_block(2, "Transcript");

    client
        .insert_document_children(&doc_id, &doc_id, 0, json!([header, meta, section]))
        .await?;

    let segments = &input.transcript.segments;
    if segments.is_empty() {
        let fallback = if input.transcript.full_text.trim().is_empty() {
            paragraph_block("(empty transcript)")
        } else {
            paragraph_block(input.transcript.full_text.trim())
        };
        client
            .insert_document_children(&doc_id, &doc_id, 3, json!([fallback]))
            .await?;
    } else {
        let mut index = 3usize;
        for chunk in segments.chunks(BATCH_SIZE) {
            let blocks: Vec<Value> = chunk
                .iter()
                .map(|s| paragraph_block(&format!("[{}] {}", format_ts(s.start_ms), s.text)))
                .collect();
            client
                .insert_document_children(&doc_id, &doc_id, index as i64, Value::Array(blocks))
                .await?;
            index += chunk.len();
        }
    }

    let files = client.list_files_in_folder(input.folder_token).await?;
    let url = files
        .into_iter()
        .find(|f| f.token == doc_id)
        .map(|f| f.url)
        .unwrap_or_default();

    Ok(TranscriptDoc { doc_id, url })
}

fn build_meta_line(duration_ms: u64, t: &Transcript, recording_url: Option<&str>) -> String {
    let mut parts = Vec::new();
    if duration_ms > 0 {
        parts.push(format!("Duration: {}", format_ts(duration_ms)));
    }
    if let Some(lang) = &t.language {
        parts.push(format!("Language: {lang}"));
    }
    if !t.segments.is_empty() {
        parts.push(format!("Segments: {}", t.segments.len()));
    }
    if let Some(url) = recording_url {
        parts.push(format!("Recording: {url}"));
    }
    if parts.is_empty() {
        "(meta unavailable)".into()
    } else {
        parts.join("  ·  ")
    }
}

fn paragraph_block(content: &str) -> Value {
    json!({
        "block_type": 2,
        "text": {
            "elements": [{
                "text_run": {
                    "content": content,
                    "text_element_style": {}
                }
            }],
            "style": {}
        }
    })
}

fn heading_block(level: u8, content: &str) -> Value {
    let (block_type, key) = match level {
        1 => (3, "heading1"),
        2 => (4, "heading2"),
        _ => (5, "heading3"),
    };
    json!({
        "block_type": block_type,
        key: {
            "elements": [{
                "text_run": {
                    "content": content,
                    "text_element_style": {}
                }
            }],
            "style": {}
        }
    })
}

fn format_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
