//! Admin-tunable runtime behavior for the linear app: which Linear events fan out
//! to subscribers, and the due-date reminder cadence/recipients. Stored as one
//! row in the shared App database (namespaced `linear_`, see [`larkstack_core::db`]);
//! when absent the code [`Settings::default`] applies. Admins edit it live via the
//! routes in [`routes`] (mounted at `/api/apps/linear/settings`) — no restart, the
//! scheduler and webhook reload per pass/event.

mod entity;
mod migration;
mod routes;

use std::time::{SystemTime, UNIX_EPOCH};

use chrono_tz::Tz;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, DbErr, EntityTrait};
use sea_orm_migration::MigrationTrait;
use tracing::warn;

pub use routes::router;

const ROW_ID: i32 = 1;

/// Schema migrations for this table.
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![Box::new(migration::Migration)]
}

/// Who receives due-date reminders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipients {
    Assignee,
    AssigneeAndSubscribers,
}

impl Recipients {
    pub fn includes_subscribers(self) -> bool {
        matches!(self, Self::AssigneeAndSubscribers)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Assignee => "assignee",
            Self::AssigneeAndSubscribers => "assignee_and_subscribers",
        }
    }

    /// Parse a wire tag; unknown values fall back to [`Recipients::Assignee`].
    fn parse(s: &str) -> Self {
        match s {
            "assignee_and_subscribers" => Self::AssigneeAndSubscribers,
            _ => Self::Assignee,
        }
    }
}

/// The decoded settings the app acts on.
#[derive(Clone, Debug)]
pub struct Settings {
    pub subscriber_on_comment: bool,
    pub subscriber_on_status_change: bool,
    pub subscriber_on_any_update: bool,
    pub reminders_enabled: bool,
    pub reminder_recipients: Recipients,
    pub reminder_lead_days: Vec<i64>,
    pub reminder_overdue_max_days: i64,
    pub reminder_check_interval_hours: u64,
    pub reminder_timezone: Tz,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            subscriber_on_comment: true,
            subscriber_on_status_change: true,
            subscriber_on_any_update: false,
            reminders_enabled: true,
            reminder_recipients: Recipients::Assignee,
            reminder_lead_days: vec![7, 3, 1, 0],
            reminder_overdue_max_days: 7,
            reminder_check_interval_hours: 6,
            reminder_timezone: Tz::UTC,
        }
    }
}

impl Settings {
    fn from_model(m: entity::Model) -> Self {
        Self {
            subscriber_on_comment: m.subscriber_on_comment,
            subscriber_on_status_change: m.subscriber_on_status_change,
            subscriber_on_any_update: m.subscriber_on_any_update,
            reminders_enabled: m.reminders_enabled,
            reminder_recipients: Recipients::parse(&m.reminder_recipients),
            reminder_lead_days: parse_lead_days(&m.reminder_lead_days),
            reminder_overdue_max_days: m.reminder_overdue_max_days.max(0) as i64,
            reminder_check_interval_hours: m.reminder_check_interval_hours.max(1) as u64,
            reminder_timezone: m.reminder_timezone.parse().unwrap_or(Tz::UTC),
        }
    }

    fn lead_days_csv(&self) -> String {
        self.reminder_lead_days
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Load the settings row, or code defaults when it's absent (or unreadable —
/// behavior degrades to defaults rather than failing the app).
pub async fn load(db: &DatabaseConnection) -> Settings {
    match entity::Entity::find_by_id(ROW_ID).one(db).await {
        Ok(Some(m)) => Settings::from_model(m),
        Ok(None) => Settings::default(),
        Err(e) => {
            warn!("settings load failed, using defaults: {e}");
            Settings::default()
        }
    }
}

/// Upsert the single settings row.
async fn save(db: &DatabaseConnection, s: &Settings) -> Result<(), DbErr> {
    let am = entity::ActiveModel {
        id: Set(ROW_ID),
        subscriber_on_comment: Set(s.subscriber_on_comment),
        subscriber_on_status_change: Set(s.subscriber_on_status_change),
        subscriber_on_any_update: Set(s.subscriber_on_any_update),
        reminders_enabled: Set(s.reminders_enabled),
        reminder_recipients: Set(s.reminder_recipients.as_str().to_string()),
        reminder_lead_days: Set(s.lead_days_csv()),
        reminder_overdue_max_days: Set(s.reminder_overdue_max_days as i32),
        reminder_check_interval_hours: Set(s.reminder_check_interval_hours as i32),
        reminder_timezone: Set(s.reminder_timezone.name().to_string()),
        updated_at: Set(now_ms()),
    };
    if entity::Entity::find_by_id(ROW_ID).one(db).await?.is_some() {
        am.update(db).await?;
    } else {
        am.insert(db).await?;
    }
    Ok(())
}

/// Parse `"7,3,1,0"` into non-negative lead days, dropping junk.
fn parse_lead_days(csv: &str) -> Vec<i64> {
    csv.split(',')
        .filter_map(|p| p.trim().parse::<i64>().ok())
        .filter(|&d| d >= 0)
        .collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
