//! This app's persistence layer — sea-orm entities + migrations backing the
//! shared App database (`larkstack_core::db`). The host runs [`migrations`] at
//! startup; every table is namespaced `linear_` as the framework requires.
//!
//! - [`user_map`] — admin Linear→Lark email overrides for DM delivery.
//! - [`settings`] — admin-tunable subscriber/reminder behavior (one row).
//! - [`due_reminders`] — dedup ledger so each reminder tier fires once.

pub mod due_reminders;
pub mod settings;
pub mod user_map;

use sea_orm_migration::MigrationTrait;

/// All of this app's schema migrations, handed to the host via `App::migrations`.
pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    let mut all = user_map::migrations();
    all.extend(settings::migrations());
    all.extend(due_reminders::migrations());
    all
}
