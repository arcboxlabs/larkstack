//! Admin-maintained Linear→Lark user mapping, for when a person's Linear account
//! email differs from their Lark email.
//!
//! The Linear webhook gives us the assignee's *Linear* email; Lark DMs are
//! addressed by *Lark* email. When they match (the common case) nothing is
//! needed. When they don't, an admin records an override here and the DM path
//! resolves through it. The table lives in the shared App database, namespaced
//! `linear_` (see [`larkstack_core::db`]); admins maintain it via the routes in
//! [`routes`], mounted at `/api/apps/linear/`.

mod entity;
mod migration;
mod routes;

use sea_orm::{DatabaseConnection, EntityTrait};
use sea_orm_migration::MigrationTrait;
use tracing::warn;

pub use routes::router;

/// Schema migrations for this App's tables.
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![Box::new(migration::Migration)]
}

/// Resolve a Linear assignee email to the Lark email to DM. Returns the override
/// when one exists, otherwise the Linear email unchanged (covers both the
/// matching-email case and any lookup failure — DM delivery degrades gracefully).
pub async fn resolve_lark_email(db: &DatabaseConnection, linear_email: &str) -> String {
    match entity::Entity::find_by_id(linear_email).one(db).await {
        Ok(Some(m)) => m.lark_email,
        Ok(None) => linear_email.to_string(),
        Err(e) => {
            warn!("user_map lookup failed for {linear_email}: {e}");
            linear_email.to_string()
        }
    }
}
