//! Creates the `linear_settings` table (`linear_` namespace, enforced by
//! [`larkstack_core::db`]). No row is seeded — the app falls back to code
//! defaults until an admin saves one.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0001_create_linear_settings"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("linear_settings"))
                    .if_not_exists()
                    .col(integer(Alias::new("id")).primary_key())
                    .col(boolean(Alias::new("subscriber_on_comment")))
                    .col(boolean(Alias::new("subscriber_on_status_change")))
                    .col(boolean(Alias::new("subscriber_on_any_update")))
                    .col(boolean(Alias::new("reminders_enabled")))
                    .col(string(Alias::new("reminder_recipients")))
                    .col(string(Alias::new("reminder_lead_days")))
                    .col(integer(Alias::new("reminder_overdue_max_days")))
                    .col(integer(Alias::new("reminder_check_interval_hours")))
                    .col(string(Alias::new("reminder_timezone")))
                    .col(big_integer(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("linear_settings"))
                    .to_owned(),
            )
            .await
    }
}
