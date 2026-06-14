//! Admin-tunable runtime behavior for the standup app: the schedule (per-job
//! trigger times + toggles + timezone), the doc table structure/wording, and the
//! minijinja message templates.
//!
//! Stored as a single JSON value in the per-App [`StateStore`] KV (namespace
//! `standup`, key `settings`) — settings are a singleton blob, never queried, so
//! the KV is a better fit than a relational table. When absent the code
//! [`Settings::default`] applies. Admins edit it live via the routes in
//! [`routes`] (mounted at `/api/apps/standup/settings`) — no restart, the
//! scheduler and bot reload each pass/command.

mod routes;

use std::sync::Arc;

use chrono_tz::{Asia::Shanghai, Tz};
use larkstack_core::StateStore;
use serde::{Deserialize, Serialize};
use tracing::warn;

pub use routes::router;

const NAMESPACE: &str = "standup";
const KEY: &str = "settings";

// Defaults — the values that were hardcoded before settings existed.
const DEFAULT_DOC_TITLE: &str = "Daily Scrum - {{ date }}";
const DEFAULT_HEADER_DONE: &str = "✅ 昨日完成";
const DEFAULT_HEADER_PLAN: &str = "🎯 今日计划";
const DEFAULT_HEADER_BLOCK: &str = "🚫 阻塞";
const DEFAULT_COLUMN_WIDTHS: [i64; 4] = [120, 300, 300, 240];

const DEFAULT_HELP: &str = "命令(群里 @ 我 或 与我私聊):
• check [today|tomorrow|YYYY-MM-DD] — 列未填者(只读)
• ensure [date] — 建文档 + 分享,不发群卡片
• announce [date] — 建文档 + 发群公告卡片
• remind [date] — 私信未填者
• urgent [date] — 加急提醒未填者
• urgent @某人 — 对指定成员加急(可 @ 多人)
• help — 本帮助
date 省略时: ensure/announce 默认明天,其余默认今天
";

const DEFAULT_CHECK: &str = "📋 {{ date }}
{{ url }}
未填写: {{ missing | length }}
{%- for item in missing %}
  - {{ item }}
{%- endfor %}
";

const DEFAULT_ANNOUNCE_TITLE: &str = "{% if days_until == 0 %}今日 {% elif days_until == 1 %}明日 {% endif %}Daily Standup · {{ date }}";

const DEFAULT_ANNOUNCE_BODY: &str = "{% if days_until == 0 %}Standup 文档已就位,如未填写请立即补上。{% elif days_until == 1 %}Standup 文档已就位。请在 **明早 10:30 之前** 完成填写。{% elif days_until > 1 %}Standup 文档已就位 ({{ days_until }} 天后),请按时填写。{% else %}Standup 文档链接见下。{% endif %}";

const DEFAULT_REMINDER_TITLE: &str =
    "{% if urgent %}⚠️ Daily Standup 最后提醒{% else %}📝 Daily Standup 提醒{% endif %}";

const DEFAULT_REMINDER_BODY: &str = "{% if urgent %}Standup 马上开始,请立刻填写你的那一行。{% else %}你还没填写今天的 Daily Standup,请尽快完成。{% endif %}";

/// One scheduled job's trigger: a wall-clock time and whether it fires.
#[derive(Clone, Copy, Debug)]
pub struct Trigger {
    pub hour: u32,
    pub minute: u32,
    pub enabled: bool,
}

impl Trigger {
    fn new(hm: &str, enabled: bool) -> Self {
        let (hour, minute) = parse_hm(hm);
        Self {
            hour,
            minute,
            enabled,
        }
    }

    /// `"HH:MM"` for the wire/storage.
    pub fn hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// The four daily jobs. The `(target day, action)` of each is intrinsic; only the
/// time + enabled flag are tunable (see [`Settings::trigger`]).
#[derive(Clone, Copy, Debug)]
pub enum Job {
    /// Announce next-day doc.
    Announce,
    /// Remind unfilled on the next-day doc (evening).
    RemindEvening,
    /// Remind unfilled on today's doc (morning).
    RemindMorning,
    /// Final reminder + in-app urgent on today's doc.
    Urgent,
}

/// The decoded settings the app acts on.
#[derive(Clone, Debug)]
pub struct Settings {
    pub timezone: Tz,
    pub announce: Trigger,
    pub remind_evening: Trigger,
    pub remind_morning: Trigger,
    pub urgent: Trigger,

    pub doc_title: String,
    pub header_done: String,
    pub header_plan: String,
    pub header_block: String,
    pub column_widths: Vec<i64>,

    pub help_template: String,
    pub check_template: String,
    pub announce_title: String,
    pub announce_body: String,
    pub reminder_title: String,
    pub reminder_body: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timezone: Shanghai,
            announce: Trigger::new("20:00", true),
            remind_evening: Trigger::new("22:00", true),
            remind_morning: Trigger::new("09:30", true),
            urgent: Trigger::new("10:00", true),
            doc_title: DEFAULT_DOC_TITLE.into(),
            header_done: DEFAULT_HEADER_DONE.into(),
            header_plan: DEFAULT_HEADER_PLAN.into(),
            header_block: DEFAULT_HEADER_BLOCK.into(),
            column_widths: DEFAULT_COLUMN_WIDTHS.to_vec(),
            help_template: DEFAULT_HELP.into(),
            check_template: DEFAULT_CHECK.into(),
            announce_title: DEFAULT_ANNOUNCE_TITLE.into(),
            announce_body: DEFAULT_ANNOUNCE_BODY.into(),
            reminder_title: DEFAULT_REMINDER_TITLE.into(),
            reminder_body: DEFAULT_REMINDER_BODY.into(),
        }
    }
}

impl Settings {
    /// The trigger for one [`Job`].
    pub fn trigger(&self, job: Job) -> Trigger {
        match job {
            Job::Announce => self.announce,
            Job::RemindEvening => self.remind_evening,
            Job::RemindMorning => self.remind_morning,
            Job::Urgent => self.urgent,
        }
    }

    /// Decode the stored/wire form, tolerating bad values (they fall back to the
    /// matching default) so a malformed blob never fails the app.
    fn from_wire(w: SettingsWire) -> Self {
        let d = Settings::default();
        Self {
            timezone: w.timezone.parse().unwrap_or(d.timezone),
            announce: Trigger::new(&w.announce_time, w.announce_enabled),
            remind_evening: Trigger::new(&w.remind_evening_time, w.remind_evening_enabled),
            remind_morning: Trigger::new(&w.remind_morning_time, w.remind_morning_enabled),
            urgent: Trigger::new(&w.urgent_time, w.urgent_enabled),
            doc_title: w.doc_title,
            header_done: w.header_done,
            header_plan: w.header_plan,
            header_block: w.header_block,
            column_widths: normalize_widths(w.column_widths),
            help_template: w.help_template,
            check_template: w.check_template,
            announce_title: w.announce_title,
            announce_body: w.announce_body,
            reminder_title: w.reminder_title,
            reminder_body: w.reminder_body,
        }
    }
}

/// Storage + wire form: times as `"HH:MM"` strings, timezone an IANA name, widths
/// a JSON array, templates raw strings — friendly for the dashboard and what's
/// persisted in the KV. Struct-level `serde(default)` means a blob written by an
/// older version (missing a field) decodes with that field's default.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SettingsWire {
    pub timezone: String,
    pub announce_time: String,
    pub announce_enabled: bool,
    pub remind_evening_time: String,
    pub remind_evening_enabled: bool,
    pub remind_morning_time: String,
    pub remind_morning_enabled: bool,
    pub urgent_time: String,
    pub urgent_enabled: bool,
    pub doc_title: String,
    pub header_done: String,
    pub header_plan: String,
    pub header_block: String,
    pub column_widths: Vec<i64>,
    pub help_template: String,
    pub check_template: String,
    pub announce_title: String,
    pub announce_body: String,
    pub reminder_title: String,
    pub reminder_body: String,
}

impl Default for SettingsWire {
    fn default() -> Self {
        (&Settings::default()).into()
    }
}

impl From<&Settings> for SettingsWire {
    fn from(s: &Settings) -> Self {
        Self {
            timezone: s.timezone.name().to_string(),
            announce_time: s.announce.hm(),
            announce_enabled: s.announce.enabled,
            remind_evening_time: s.remind_evening.hm(),
            remind_evening_enabled: s.remind_evening.enabled,
            remind_morning_time: s.remind_morning.hm(),
            remind_morning_enabled: s.remind_morning.enabled,
            urgent_time: s.urgent.hm(),
            urgent_enabled: s.urgent.enabled,
            doc_title: s.doc_title.clone(),
            header_done: s.header_done.clone(),
            header_plan: s.header_plan.clone(),
            header_block: s.header_block.clone(),
            column_widths: s.column_widths.clone(),
            help_template: s.help_template.clone(),
            check_template: s.check_template.clone(),
            announce_title: s.announce_title.clone(),
            announce_body: s.announce_body.clone(),
            reminder_title: s.reminder_title.clone(),
            reminder_body: s.reminder_body.clone(),
        }
    }
}

/// Load the settings blob, or code defaults when it's absent (or unreadable —
/// behavior degrades to defaults rather than failing the app).
pub async fn load(store: &Arc<dyn StateStore>) -> Settings {
    match store.get(NAMESPACE, KEY).await {
        Ok(Some(json)) => match serde_json::from_str::<SettingsWire>(&json) {
            Ok(w) => Settings::from_wire(w),
            Err(e) => {
                warn!("standup settings parse failed, using defaults: {e}");
                Settings::default()
            }
        },
        Ok(None) => Settings::default(),
        Err(e) => {
            warn!("standup settings load failed, using defaults: {e}");
            Settings::default()
        }
    }
}

/// Persist the (already-validated) wire form.
async fn save(store: &Arc<dyn StateStore>, wire: &SettingsWire) -> anyhow::Result<()> {
    let json = serde_json::to_string(wire)?;
    store.set(NAMESPACE, KEY, &json).await
}

/// Parse `"HH:MM"` into `(hour, minute)`; junk → `00:00`.
fn parse_hm(s: &str) -> (u32, u32) {
    let mut parts = s.splitn(2, ':');
    let h = parts.next().and_then(|p| p.trim().parse::<u32>().ok());
    let m = parts.next().and_then(|p| p.trim().parse::<u32>().ok());
    match (h, m) {
        (Some(h), Some(m)) if h < 24 && m < 60 => (h, m),
        _ => (0, 0),
    }
}

/// Drop non-positive widths; empty → defaults.
fn normalize_widths(widths: Vec<i64>) -> Vec<i64> {
    let v: Vec<i64> = widths.into_iter().filter(|&w| w > 0).collect();
    if v.is_empty() {
        DEFAULT_COLUMN_WIDTHS.to_vec()
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    /// The default templates carry conditionals/loops that are only evaluated at
    /// runtime — render each so a syntax slip in a default is caught at test time
    /// (a render error is returned inline as `"[template error: …]"`).
    #[test]
    fn default_templates_render() {
        let s = Settings::default();
        let assert_ok = |out: String| {
            assert!(!out.contains("[template error"), "render failed: {out}");
            out
        };

        assert_ok(crate::template::render(&s.help_template, context! {}));
        assert_ok(crate::template::render(
            &s.check_template,
            context! { date => "2026-06-14", url => "u", missing => vec!["a", "b"] },
        ));
        assert_ok(crate::template::render(
            &s.doc_title,
            context! { date => "2026-06-14" },
        ));

        // Announce wording branches on how far off the date is.
        for days_until in [-1_i64, 0, 1, 5] {
            let title = assert_ok(crate::template::render(
                &s.announce_title,
                context! { date => "2026-06-14", days_until },
            ));
            assert!(title.contains("Daily Standup"));
            assert_ok(crate::template::render(
                &s.announce_body,
                context! { date => "2026-06-14", days_until, url => "u" },
            ));
        }

        // Reminder wording branches on urgency.
        for urgent in [true, false] {
            assert_ok(crate::template::render(
                &s.reminder_title,
                context! { urgent },
            ));
            assert_ok(crate::template::render(
                &s.reminder_body,
                context! { urgent, url => "u" },
            ));
        }
    }

    /// A stored blob missing newer fields still decodes (struct-level serde
    /// default), filling the gaps from `Settings::default`.
    #[test]
    fn partial_blob_decodes_with_defaults() {
        let json = r#"{"announce_time":"08:30","announce_enabled":false}"#;
        let wire: SettingsWire = serde_json::from_str(json).expect("partial decode");
        let s = Settings::from_wire(wire);
        assert_eq!(s.announce.hm(), "08:30");
        assert!(!s.announce.enabled);
        // Untouched fields fall back to defaults.
        assert_eq!(s.remind_evening.hm(), "22:00");
        assert_eq!(s.timezone, Shanghai);
        assert_eq!(s.column_widths, DEFAULT_COLUMN_WIDTHS.to_vec());
    }
}
