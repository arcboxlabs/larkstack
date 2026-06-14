//! The standup doc itself: a per-day Lark Docx with one table whose rows are
//! chat members. Creates the doc if missing, seeds the header + member rows,
//! and reads it back to detect who still hasn't filled their cells.

use std::collections::HashMap;

use chrono::NaiveDate;
use larkoapi::{ChatMember, LarkBotClient};
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::config::StandupConfig;

const HEADER_DONE: &str = "✅ 昨日完成";
const HEADER_PLAN: &str = "🎯 今日计划";
const HEADER_BLOCK: &str = "🚫 阻塞";
const COL_SIZE: usize = 4;
const COLUMN_WIDTH: [i64; COL_SIZE] = [120, 300, 300, 240];

pub struct StandupDoc {
    pub doc_id: String,
    pub url: String,
}

fn document_title(date: NaiveDate) -> String {
    format!("Daily Scrum - {}", date.format("%Y-%m-%d"))
}

pub async fn ensure_document_for_date(
    client: &LarkBotClient,
    cfg: &StandupConfig,
    date: NaiveDate,
) -> Result<StandupDoc, String> {
    let folder = cfg
        .folder_token
        .as_deref()
        .ok_or("STANDUP_FOLDER_TOKEN not set")?;
    let chat_id = cfg.chat_id.as_deref().ok_or("STANDUP_CHAT_ID not set")?;
    let title = document_title(date);

    let files = client.list_files_in_folder(folder).await?;
    if let Some(existing) = files
        .iter()
        .find(|f| f.file_type == "docx" && f.name == title)
    {
        return Ok(StandupDoc {
            doc_id: existing.token.clone(),
            url: existing.url.clone(),
        });
    }

    let members = client.list_chat_members(chat_id).await?;
    if members.is_empty() {
        return Err(format!("chat {chat_id} has no members"));
    }

    let doc_id = client.create_docx_in_folder(folder, &title).await?;
    info!("standup: created doc {doc_id} for {date}");
    populate_standup_table(client, &doc_id, &members).await?;
    if let Err(e) = client.share_file_with_chat(&doc_id, "docx", chat_id).await {
        warn!("standup: share doc {doc_id} to chat failed: {e}");
    }

    let files_after = client.list_files_in_folder(folder).await?;
    let url = files_after
        .into_iter()
        .find(|f| f.token == doc_id)
        .map(|f| f.url)
        .unwrap_or_default();

    Ok(StandupDoc { doc_id, url })
}

async fn populate_standup_table(
    client: &LarkBotClient,
    doc_id: &str,
    members: &[ChatMember],
) -> Result<(), String> {
    let row_size = (members.len() + 1) as i64;
    let children = json!([{
        "block_type": 31,
        "table": {
            "property": {
                "row_size": row_size,
                "column_size": COL_SIZE as i64,
                "column_width": COLUMN_WIDTH,
                "header_row": true
            }
        }
    }]);

    // Column widths only apply at creation; Lark's PATCH set_column_width silently no-ops.
    let _ = client
        .insert_document_children(doc_id, doc_id, 0, children)
        .await?;

    let blocks = client.list_document_blocks(doc_id).await?;
    let table_block = blocks
        .iter()
        .find(|b| b.get("block_type").and_then(|v| v.as_i64()) == Some(31))
        .ok_or("no table block after creation")?;
    let cells: Vec<&str> = table_block
        .pointer("/table/cells")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if cells.len() != row_size as usize * COL_SIZE {
        return Err(format!(
            "unexpected cell count {} (want {})",
            cells.len(),
            row_size as usize * COL_SIZE
        ));
    }

    let cell_to_text = cell_text_map(&blocks);

    let headers = ["", HEADER_DONE, HEADER_PLAN, HEADER_BLOCK];
    let mut requests: Vec<Value> = Vec::new();
    for (col, header) in headers.iter().enumerate() {
        if header.is_empty() {
            continue;
        }
        let text_id = cell_to_text
            .get(cells[col])
            .ok_or_else(|| format!("header text block missing for col {col}"))?;
        requests.push(json!({
            "block_id": text_id,
            "update_text_elements": {
                "elements": [{
                    "text_run": {
                        "content": header,
                        "text_element_style": {"bold": true}
                    }
                }]
            }
        }));
    }
    for (i, m) in members.iter().enumerate() {
        let row = i + 1;
        let text_id = cell_to_text
            .get(cells[row * COL_SIZE])
            .ok_or_else(|| format!("name text block missing for row {row}"))?;
        requests.push(json!({
            "block_id": text_id,
            "update_text_elements": {
                "elements": [{
                    "mention_user": {
                        "user_id": m.member_id,
                        "text_element_style": {}
                    }
                }]
            }
        }));
    }

    client
        .batch_update_document_blocks(doc_id, Value::Array(requests))
        .await?;
    Ok(())
}

pub async fn find_missing_user_ids(
    client: &LarkBotClient,
    doc_id: &str,
) -> Result<Vec<String>, String> {
    let blocks = client.list_document_blocks(doc_id).await?;
    let by_id: HashMap<&str, &Value> = blocks
        .iter()
        .filter_map(|b| b.get("block_id").and_then(|v| v.as_str()).map(|id| (id, b)))
        .collect();

    let table = blocks
        .iter()
        .find(|b| b.get("block_type").and_then(|v| v.as_i64()) == Some(31))
        .ok_or("no table block found")?;
    let cells: Vec<&str> = table
        .pointer("/table/cells")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if cells.is_empty() || !cells.len().is_multiple_of(COL_SIZE) {
        return Err(format!("unexpected cells count: {}", cells.len()));
    }

    let row_size = cells.len() / COL_SIZE;
    let cell_to_text = cell_text_map(&blocks);

    let mut missing = Vec::new();
    for row in 1..row_size {
        let name_cell = cells[row * COL_SIZE];
        let Some(name_text_id) = cell_to_text.get(name_cell) else {
            continue;
        };
        let Some(name_block) = by_id.get(name_text_id.as_str()) else {
            continue;
        };
        let Some(user_id) = extract_mention_user(name_block) else {
            continue;
        };

        let all_empty = (1..COL_SIZE).all(|col| {
            let cid = cells[row * COL_SIZE + col];
            let Some(tid) = cell_to_text.get(cid) else {
                return true;
            };
            let Some(blk) = by_id.get(tid.as_str()) else {
                return true;
            };
            is_text_empty(blk)
        });

        if all_empty {
            missing.push(user_id);
        }
    }
    Ok(missing)
}

fn cell_text_map(blocks: &[Value]) -> HashMap<String, String> {
    let by_id: HashMap<&str, &Value> = blocks
        .iter()
        .filter_map(|b| b.get("block_id").and_then(|v| v.as_str()).map(|id| (id, b)))
        .collect();
    let mut map = HashMap::new();
    for b in blocks {
        if b.get("block_type").and_then(|v| v.as_i64()) != Some(2) {
            continue;
        }
        let Some(text_id) = b.get("block_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(parent_id) = b.get("parent_id").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(parent) = by_id.get(parent_id)
            && parent.get("block_type").and_then(|v| v.as_i64()) == Some(32)
        {
            map.insert(parent_id.to_string(), text_id.to_string());
        }
    }
    map
}

fn extract_mention_user(text_block: &Value) -> Option<String> {
    let elements = text_block.pointer("/text/elements")?.as_array()?;
    for el in elements {
        if let Some(uid) = el.pointer("/mention_user/user_id").and_then(|v| v.as_str()) {
            return Some(uid.to_string());
        }
    }
    None
}

fn is_text_empty(text_block: &Value) -> bool {
    let Some(elements) = text_block
        .pointer("/text/elements")
        .and_then(|v| v.as_array())
    else {
        return true;
    };
    if elements.is_empty() {
        return true;
    }
    elements.iter().all(|el| {
        el.pointer("/text_run/content")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().is_empty())
            .unwrap_or(false)
    })
}
