//! `linear_settings` entity: the single-row, admin-tunable runtime behavior for
//! the linear app (subscriber fan-out scope + due-date reminder cadence).

use sea_orm::entity::prelude::*;

/// The one settings row (`id` is always `1`). Lists and enums are stored as
/// strings (`reminder_lead_days` = `"7,3,1,0"`, `reminder_recipients` = an enum
/// tag) and parsed in [`super`].
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "linear_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i32,
    pub subscriber_on_comment: bool,
    pub subscriber_on_status_change: bool,
    pub subscriber_on_any_update: bool,
    pub reminders_enabled: bool,
    pub reminder_recipients: String,
    pub reminder_lead_days: String,
    pub reminder_overdue_max_days: i32,
    pub reminder_check_interval_hours: i32,
    pub reminder_timezone: String,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
