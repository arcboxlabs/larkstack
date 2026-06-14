//! Creates the `linear_due_reminders` dedup table (`linear_` namespace, enforced
//! by [`larkstack_core::db`]) with a composite `(issue_id, due_date, tier)` key.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0001_create_linear_due_reminders"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("linear_due_reminders"))
                    .if_not_exists()
                    .col(string(Alias::new("issue_id")))
                    .col(string(Alias::new("due_date")))
                    .col(integer(Alias::new("tier")))
                    .col(big_integer(Alias::new("sent_at")))
                    .primary_key(
                        Index::create()
                            .col(Alias::new("issue_id"))
                            .col(Alias::new("due_date"))
                            .col(Alias::new("tier")),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("linear_due_reminders"))
                    .to_owned(),
            )
            .await
    }
}
