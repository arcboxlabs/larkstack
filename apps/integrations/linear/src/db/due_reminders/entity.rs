//! `linear_due_reminders` entity: one row per reminder already sent, keyed by
//! `(issue_id, due_date, tier)` so each cadence tier fires at most once per
//! deadline. Re-keying on `due_date` means moving an issue's deadline naturally
//! re-arms its tiers.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "linear_due_reminders")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub issue_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub due_date: String,
    /// Cadence tier: a positive lead-day for scheduled reminders, a negative day
    /// count for overdue ones (`-1, -2, …`).
    #[sea_orm(primary_key, auto_increment = false)]
    pub tier: i32,
    pub sent_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
