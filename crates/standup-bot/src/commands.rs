//! Lark bot command handler — listens on the WebSocket long connection and
//! interprets `@bot <cmd>` in groups or direct `<cmd>` in p2p chats.

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use larkoapi::{LarkBotClient, WsEventHandler};
use serde_json::Value;
use tracing::{info, warn};

use crate::config::StandupConfig;
use crate::flow;
use crate::templates::{CheckTemplate, HelpTemplate};

pub struct CommandBot {
    pub cfg: Arc<StandupConfig>,
    pub client: Arc<LarkBotClient>,
    pub bot_open_id: String,
}

#[async_trait]
impl WsEventHandler for CommandBot {
    async fn handle_event(&self, event: &Value) -> Option<Value> {
        let event_type = event
            .pointer("/header/event_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if event_type != "im.message.receive_v1" {
            return None;
        }

        let msg = event.pointer("/event/message")?;
        let chat_type = msg.get("chat_type").and_then(|v| v.as_str()).unwrap_or("");
        let msg_type = msg
            .get("message_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if msg_type != "text" {
            return None;
        }

        let sender_open_id = event
            .pointer("/event/sender/sender_id/open_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let chat_id = msg
            .get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let content_str = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let content: Value = serde_json::from_str(content_str).unwrap_or(Value::Null);
        let raw_text = content
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mentions: Vec<Value> = msg
            .get("mentions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let cmd_text = match chat_type {
            "group" => {
                let bot_mentioned = mentions.iter().any(|m| {
                    m.pointer("/id/open_id").and_then(|v| v.as_str()) == Some(&self.bot_open_id)
                });
                if !bot_mentioned {
                    return None;
                }
                let mut t = raw_text.clone();
                for m in &mentions {
                    if let Some(key) = m.get("key").and_then(|v| v.as_str()) {
                        // Strip only the bot's @ placeholder; leave other mentions intact
                        // so commands like `urgent @张煊` can still extract targets.
                        if m.pointer("/id/open_id").and_then(|v| v.as_str())
                            == Some(&self.bot_open_id)
                        {
                            t = t.replace(key, "");
                        }
                    }
                }
                t.trim().to_string()
            }
            "p2p" => raw_text.trim().to_string(),
            other => {
                warn!("standup[cmd]: unsupported chat_type {other}, ignoring");
                return None;
            }
        };

        info!(
            "standup[cmd] chat={chat_id} type={chat_type} from={sender_open_id} text={cmd_text:?}"
        );

        let reply = match self.dispatch(&cmd_text, &mentions).await {
            Ok(r) => r,
            Err(e) => format!("❌ {e}"),
        };
        if let Err(e) = self.client.send_text(&chat_id, "chat_id", &reply).await {
            warn!("standup[cmd]: reply failed: {e}");
        }
        None
    }
}

impl CommandBot {
    async fn dispatch(&self, cmd_text: &str, mentions: &[Value]) -> Result<String, String> {
        let tokens: Vec<&str> = cmd_text.split_whitespace().collect();
        let cmd = tokens.first().copied().unwrap_or("help");
        let second = tokens.get(1).copied();
        let today = Utc::now().with_timezone(&Shanghai).date_naive();
        let tomorrow = today + ChronoDuration::days(1);

        // non-bot mentioned open_ids (for `urgent @user`)
        let mentioned_users: Vec<String> = mentions
            .iter()
            .filter_map(|m| {
                m.pointer("/id/open_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .filter(|id| id != &self.bot_open_id)
            .collect();

        match cmd {
            "help" | "/help" | "h" | "?" => Ok(render_help()),
            "check" | "/check" => {
                let date = resolve_date(second, today);
                self.do_check(date).await
            }
            "ensure" | "/ensure" => {
                let date = resolve_date(second, tomorrow);
                let doc = flow::ensure_document_for_date(&self.client, &self.cfg, date).await?;
                Ok(format!("✅ {date} 文档已就位\n{}", doc.url))
            }
            "announce" | "/announce" => {
                let date = resolve_date(second, tomorrow);
                flow::announce(&self.cfg, &self.client, date).await?;
                Ok(format!("✅ {date} 公告已发群"))
            }
            "remind" | "/remind" => {
                let date = resolve_date(second, today);
                flow::remind(&self.cfg, &self.client, date, false).await?;
                Ok(format!("✅ {date} 提醒已发送(未填者)"))
            }
            "urgent" | "/urgent" => {
                if !mentioned_users.is_empty() {
                    let mut out = String::new();
                    for oid in &mentioned_users {
                        match flow::urgent_one(&self.cfg, &self.client, today, oid).await {
                            Ok(()) => out.push_str(&format!("✅ 已加急 → {oid}\n")),
                            Err(e) => out.push_str(&format!("❌ {oid}: {e}\n")),
                        }
                    }
                    Ok(out.trim_end().to_string())
                } else {
                    let date = resolve_date(second, today);
                    flow::remind(&self.cfg, &self.client, date, true).await?;
                    Ok(format!("✅ {date} 加急提醒已发出(未填者)"))
                }
            }
            other => Err(format!("未知命令: {other}\n\n{}", render_help())),
        }
    }

    async fn do_check(&self, date: NaiveDate) -> Result<String, String> {
        let doc = flow::ensure_document_for_date(&self.client, &self.cfg, date).await?;
        let missing = flow::find_missing_user_ids(&self.client, &doc.doc_id).await?;
        let mut name_of: HashMap<String, String> = HashMap::new();
        if let Some(chat_id) = self.cfg.chat_id.as_deref()
            && let Ok(members) = self.client.list_chat_members(chat_id).await
        {
            for m in members {
                name_of.insert(m.member_id, m.name);
            }
        }
        let rows: Vec<String> = missing
            .iter()
            .map(|uid| {
                let name = name_of.get(uid).cloned().unwrap_or_default();
                if name.is_empty() { uid.clone() } else { name }
            })
            .collect();
        CheckTemplate {
            date: &date.to_string(),
            url: &doc.url,
            missing: rows,
        }
        .render()
        .map_err(|e| format!("render check: {e}"))
    }
}

fn render_help() -> String {
    HelpTemplate
        .render()
        .unwrap_or_else(|e| format!("help render failed: {e}"))
}

fn resolve_date(arg: Option<&str>, default: NaiveDate) -> NaiveDate {
    match arg {
        None => default,
        Some("today") => Utc::now().with_timezone(&Shanghai).date_naive(),
        Some("tomorrow") => {
            Utc::now().with_timezone(&Shanghai).date_naive() + ChronoDuration::days(1)
        }
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or(default),
    }
}
