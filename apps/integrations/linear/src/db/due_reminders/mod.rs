//! Dedup ledger for due-date reminders: records which cadence tier has been sent
//! for each `(issue, deadline)` so the scheduler fires each at most once. See
//! [`crate::domain::reminders`] for the cadence and [`crate::scheduler`] for the
//! pass that consults this.

mod entity;
mod migration;

use std::time::{SystemTime, UNIX_EPOCH};

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, DbErr, EntityTrait};
use sea_orm_migration::MigrationTrait;
use tracing::warn;

/// Schema migrations for this table.
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![Box::new(migration::Migration)]
}

/// Whether the `tier` reminder for `(issue_id, due_date)` has already been sent.
/// On a lookup error returns `true` (treat as sent) so a flaky DB never causes
/// duplicate DMs.
pub async fn already_sent(
    db: &DatabaseConnection,
    issue_id: &str,
    due_date: &str,
    tier: i32,
) -> bool {
    match entity::Entity::find_by_id((issue_id.to_string(), due_date.to_string(), tier))
        .one(db)
        .await
    {
        Ok(opt) => opt.is_some(),
        Err(e) => {
            warn!("due_reminders lookup failed for {issue_id}@{due_date} tier {tier}: {e}");
            true
        }
    }
}

/// Record that the `tier` reminder for `(issue_id, due_date)` was sent.
pub async fn record(
    db: &DatabaseConnection,
    issue_id: &str,
    due_date: &str,
    tier: i32,
) -> Result<(), DbErr> {
    entity::ActiveModel {
        issue_id: Set(issue_id.to_string()),
        due_date: Set(due_date.to_string()),
        tier: Set(tier),
        sent_at: Set(now_ms()),
    }
    .insert(db)
    .await
    .map(|_| ())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
