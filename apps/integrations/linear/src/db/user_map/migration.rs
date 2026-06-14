//! Creates the `linear_user_map` table. The name carries the required `linear_`
//! namespace prefix the framework enforces (see [`larkstack_core::db`]).

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0001_create_linear_user_map"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("linear_user_map"))
                    .if_not_exists()
                    .col(string(Alias::new("linear_email")).primary_key())
                    .col(string(Alias::new("lark_email")))
                    .col(string_null(Alias::new("lark_open_id")))
                    .col(string_null(Alias::new("note")))
                    .col(string_null(Alias::new("updated_by")))
                    .col(big_integer(Alias::new("updated_at")))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("linear_user_map"))
                    .to_owned(),
            )
            .await
    }
}
