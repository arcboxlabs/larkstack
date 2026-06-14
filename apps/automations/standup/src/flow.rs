//! Per-job orchestration: ensure the daily doc exists, find who hasn't filled
//! it, and fan out the announce/reminder/urgent cards. The Lark mechanics live
//! in [`crate::lark`]; this module is the high-level standup operations the CLI,
//! console actions, scheduler, and chat bot all dispatch to.

use std::collections::HashMap;

use askama::Template;
use chrono::NaiveDate;
use larkoapi::LarkBotClient;
use tracing::{error, info};

use crate::config::StandupConfig;
use crate::lark::card::{build_announce_card, build_reminder_card};
use crate::lark::doc::{ensure_document_for_date, find_missing_user_ids};
use crate::templates::CheckTemplate;

/// Build the doc (if missing) + share with chat. No chat announcement card.
pub async fn ensure(
    cfg: &StandupConfig,
    client: &LarkBotClient,
    date: NaiveDate,
) -> Result<(), String> {
    let doc = ensure_document_for_date(client, cfg, date).await?;
    info!("standup: ensured {date} doc={} url={}", doc.doc_id, doc.url);
    println!("{date}\t{}\t{}", doc.doc_id, doc.url);
    Ok(())
}

/// Ensure doc + send announcement card to the chat.
pub async fn announce(
    cfg: &StandupConfig,
    client: &LarkBotClient,
    date: NaiveDate,
) -> Result<(), String> {
    let chat_id = cfg.chat_id.as_deref().ok_or("chat_id missing")?;
    let doc = ensure_document_for_date(client, cfg, date).await?;
    let card = build_announce_card(&doc.url, date);
    client
        .send_interactive_returning_id(chat_id, "chat_id", &card)
        .await?;
    info!("standup: announced {date} -> {}", doc.doc_id);
    Ok(())
}

/// DM every member whose cells are still empty. When `urgent`, follow up with
/// the in-app urgent escalation on the same message.
pub async fn remind(
    cfg: &StandupConfig,
    client: &LarkBotClient,
    date: NaiveDate,
    urgent: bool,
) -> Result<(), String> {
    let doc = ensure_document_for_date(client, cfg, date).await?;
    let missing = find_missing_user_ids(client, &doc.doc_id).await?;
    if missing.is_empty() {
        info!("standup: {date} fully filled, skipping reminder (urgent={urgent})");
        return Ok(());
    }
    info!(
        "standup: {date} missing {} user(s), urgent={urgent}",
        missing.len()
    );
    let card = build_reminder_card(&doc.url, urgent);
    let mut delivered: Vec<(String, String)> = Vec::new();
    for uid in &missing {
        match client
            .send_interactive_returning_id(uid, "open_id", &card)
            .await
        {
            Ok(mid) => delivered.push((uid.clone(), mid)),
            Err(e) => error!("standup: DM to {uid} failed: {e}"),
        }
    }
    if urgent {
        for (uid, mid) in delivered {
            if let Err(e) = client.urgent_app(&mid, std::slice::from_ref(&uid)).await {
                error!("standup: urgent {mid} for {uid} failed: {e}");
            }
        }
    }
    Ok(())
}

/// Send one reminder + in-app urgent to a specific open_id, regardless of fill state.
/// Useful for manual testing and ad-hoc escalation.
pub async fn urgent_one(
    cfg: &StandupConfig,
    client: &LarkBotClient,
    date: NaiveDate,
    open_id: &str,
) -> Result<(), String> {
    let doc = ensure_document_for_date(client, cfg, date).await?;
    let card = build_reminder_card(&doc.url, true);
    let mid = client
        .send_interactive_returning_id(open_id, "open_id", &card)
        .await?;
    info!("standup: urgent DM sent to {open_id} -> message_id={mid}");
    client
        .urgent_app(&mid, std::slice::from_ref(&open_id.to_string()))
        .await?;
    info!("standup: urgent_app fired for {open_id}");
    println!("ok: DM + urgent sent to {open_id} (message_id={mid})");
    Ok(())
}

/// Read-only check: print who hasn't filled the doc for `date`.
pub async fn check(
    cfg: &StandupConfig,
    client: &LarkBotClient,
    date: NaiveDate,
) -> Result<(), String> {
    let doc = ensure_document_for_date(client, cfg, date).await?;
    let missing = find_missing_user_ids(client, &doc.doc_id).await?;

    let mut name_of: HashMap<String, String> = HashMap::new();
    if let Some(chat_id) = cfg.chat_id.as_deref()
        && let Ok(members) = client.list_chat_members(chat_id).await
    {
        for m in members {
            name_of.insert(m.member_id, m.name);
        }
    }

    let rows: Vec<String> = missing
        .iter()
        .map(|uid| {
            let name = name_of.get(uid).cloned().unwrap_or_default();
            if name.is_empty() {
                uid.clone()
            } else {
                format!("{name} ({uid})")
            }
        })
        .collect();
    let rendered = CheckTemplate {
        date: &date.to_string(),
        url: &doc.url,
        missing: rows,
    }
    .render()
    .map_err(|e| format!("render check: {e}"))?;
    println!("doc_id:  {}", doc.doc_id);
    print!("{rendered}");
    Ok(())
}
