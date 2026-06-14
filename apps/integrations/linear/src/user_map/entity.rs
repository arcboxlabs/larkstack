//! `linear_user_map` entity: admin-maintained Linear-email → Lark-email overrides.

use sea_orm::entity::prelude::*;

/// One override row. The Linear account email is the key; `lark_email` is where
/// the assignee DM should actually go when the two systems use different
/// addresses. `lark_open_id` is reserved for future id-based delivery (e.g.
/// urgent notifications); the DM path uses the email.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "linear_user_map")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub linear_email: String,
    pub lark_email: String,
    pub lark_open_id: Option<String>,
    pub note: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
