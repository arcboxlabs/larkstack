//! Per-App relational storage: a shared SQLite database (`apps.db`) where each
//! App owns a set of tables, created through [`sea_orm_migration`] migrations the
//! App declares via [`App::migrations`](crate::App::migrations).
//!
//! Apps share one database, so table names are **namespaced by App**: every table
//! an App creates must be prefixed `"<app>_"`. The framework owns the migration
//! runner ([`run_migrations`]) — not the App — which is what makes the prefix
//! *enforced* rather than merely conventional: each migration runs inside a
//! transaction, and if it creates (or drops) a table outside the App's namespace
//! the transaction is rolled back and the App fails to start.
//!
//! Applied migrations are tracked in a single framework-owned table keyed by
//! `(app, name)`, so every App's migrations coexist in one database without the
//! per-`Migrator` `seaql_migrations` collisions that sea-orm's own runner would
//! hit when several Apps share a connection.
//!
//! Caveat of the shared-database model: a migration that *alters* (rather than
//! creates/drops) another App's table cannot be detected from the table list and
//! is not blocked. Creation and deletion across namespaces are.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, TransactionTrait, Value,
};
use sea_orm_migration::{MigrationTrait, SchemaManager};
use tracing::info;

pub use sea_orm;
pub use sea_orm_migration;

/// Framework-owned table tracking which `(app, migration)` pairs are applied.
/// Underscore-prefixed and exempt from the per-App namespace check.
const TRACKING_TABLE: &str = "_larkstack_migrations";

/// The required table-name prefix for an App's tables (`"linear_"` etc.).
pub fn table_prefix(app: &str) -> String {
    format!("{app}_")
}

/// Namespace a table name under its App: `table_name("linear", "user_map")` →
/// `"linear_user_map"`. Migrations should build table identifiers through this
/// so the name matches what [`run_migrations`] enforces.
pub fn table_name(app: &str, name: &str) -> String {
    format!("{}{name}", table_prefix(app))
}

/// Open (creating if absent) the shared App database at `path` in WAL mode.
///
/// Uses a single connection: App-table traffic is low-volume (admin edits,
/// occasional lookups) and serializing it sidesteps SQLite write-lock contention.
pub async fn open(path: impl AsRef<Path>) -> anyhow::Result<DatabaseConnection> {
    let path = path.as_ref();
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("resolve current dir for app db path")?
            .join(path)
    };
    let url = format!("sqlite://{}?mode=rwc", abs.display());

    let mut opt = sea_orm::ConnectOptions::new(url);
    opt.max_connections(1).sqlx_logging(false);
    let db = Database::connect(opt)
        .await
        .with_context(|| format!("open app db at {}", abs.display()))?;

    db.execute_unprepared("PRAGMA journal_mode=WAL;").await?;
    db.execute_unprepared("PRAGMA busy_timeout=5000;").await?;
    Ok(db)
}

/// Apply `app`'s pending migrations against the shared database.
///
/// Idempotent: already-applied migrations (per the tracking table) are skipped,
/// so it is safe to call on every App (re)start. Each pending migration runs in
/// its own transaction with the namespace check; a violating or failing
/// migration rolls back and returns `Err` (the supervisor surfaces it as the
/// App erroring, with backoff).
pub async fn run_migrations(
    db: &DatabaseConnection,
    app: &str,
    migrations: Vec<Box<dyn MigrationTrait>>,
) -> anyhow::Result<()> {
    ensure_tracking_table(db).await?;
    for migration in migrations {
        let name = migration.name().to_string();
        if is_applied(db, app, &name).await? {
            continue;
        }
        apply_one(db, app, &name, migration.as_ref())
            .await
            .with_context(|| format!("app '{app}' migration '{name}'"))?;
        info!(app, migration = %name, "applied app migration");
    }
    Ok(())
}

async fn ensure_tracking_table(db: &DatabaseConnection) -> anyhow::Result<()> {
    db.execute_unprepared(&format!(
        "CREATE TABLE IF NOT EXISTS {TRACKING_TABLE} (\
            app TEXT NOT NULL, \
            name TEXT NOT NULL, \
            applied_at INTEGER NOT NULL, \
            PRIMARY KEY (app, name))"
    ))
    .await?;
    Ok(())
}

async fn is_applied(db: &DatabaseConnection, app: &str, name: &str) -> anyhow::Result<bool> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!("SELECT 1 FROM {TRACKING_TABLE} WHERE app = ? AND name = ? LIMIT 1"),
        [Value::from(app), Value::from(name)],
    );
    Ok(db.query_one_raw(stmt).await?.is_some())
}

/// Run one migration transactionally, enforcing the App's table namespace before
/// committing.
async fn apply_one(
    db: &DatabaseConnection,
    app: &str,
    name: &str,
    migration: &dyn MigrationTrait,
) -> anyhow::Result<()> {
    let txn = db.begin().await?;

    let before = table_set(&txn).await?;
    {
        let manager = SchemaManager::new(&txn);
        migration.up(&manager).await?;
    }
    let after = table_set(&txn).await?;

    let prefix = table_prefix(app);
    for created in after.difference(&before) {
        if !is_framework_table(created) && !created.starts_with(&prefix) {
            bail!("created table '{created}' outside required '{prefix}' namespace");
        }
    }
    for dropped in before.difference(&after) {
        if !is_framework_table(dropped) && !dropped.starts_with(&prefix) {
            bail!("dropped table '{dropped}' outside '{prefix}' namespace");
        }
    }

    let insert = Statement::from_sql_and_values(
        DbBackend::Sqlite,
        format!("INSERT INTO {TRACKING_TABLE} (app, name, applied_at) VALUES (?, ?, ?)"),
        [Value::from(app), Value::from(name), Value::from(now_ms())],
    );
    txn.execute_raw(insert).await?;
    txn.commit().await?;
    Ok(())
}

/// The set of user table names currently in the database.
async fn table_set<C: ConnectionTrait>(conn: &C) -> anyhow::Result<BTreeSet<String>> {
    let stmt = Statement::from_string(
        DbBackend::Sqlite,
        "SELECT name FROM sqlite_master WHERE type = 'table'",
    );
    let mut set = BTreeSet::new();
    for row in conn.query_all_raw(stmt).await? {
        set.insert(row.try_get::<String>("", "name")?);
    }
    Ok(set)
}

/// Tables the namespace check ignores: the tracking table and SQLite internals.
fn is_framework_table(name: &str) -> bool {
    name == TRACKING_TABLE || name.starts_with("sqlite_")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::MigrationName;
    use sea_orm_migration::async_trait::async_trait;

    /// A migration creating `<app>_things` — well-behaved.
    struct GoodMigration;
    impl MigrationName for GoodMigration {
        fn name(&self) -> &str {
            "m_good"
        }
    }
    #[async_trait]
    impl MigrationTrait for GoodMigration {
        async fn up(&self, manager: &SchemaManager) -> Result<(), sea_orm::DbErr> {
            manager
                .get_connection()
                .execute_unprepared("CREATE TABLE testapp_things (id INTEGER PRIMARY KEY)")
                .await?;
            Ok(())
        }
    }

    /// A migration creating an unprefixed `rogue` table — must be rejected.
    struct RogueMigration;
    impl MigrationName for RogueMigration {
        fn name(&self) -> &str {
            "m_rogue"
        }
    }
    #[async_trait]
    impl MigrationTrait for RogueMigration {
        async fn up(&self, manager: &SchemaManager) -> Result<(), sea_orm::DbErr> {
            manager
                .get_connection()
                .execute_unprepared("CREATE TABLE rogue (id INTEGER PRIMARY KEY)")
                .await?;
            Ok(())
        }
    }

    async fn mem_db() -> DatabaseConnection {
        // A private in-memory database for the test.
        Database::connect("sqlite::memory:").await.unwrap()
    }

    async fn has_table(db: &DatabaseConnection, name: &str) -> bool {
        table_set(db).await.unwrap().contains(name)
    }

    #[tokio::test]
    async fn applies_prefixed_migration_and_is_idempotent() {
        let db = mem_db().await;
        run_migrations(&db, "testapp", vec![Box::new(GoodMigration)])
            .await
            .unwrap();
        assert!(has_table(&db, "testapp_things").await);

        // Re-running is a no-op (tracked), not a "table already exists" error.
        run_migrations(&db, "testapp", vec![Box::new(GoodMigration)])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejects_unprefixed_table_and_rolls_back() {
        let db = mem_db().await;
        let err = run_migrations(&db, "testapp", vec![Box::new(RogueMigration)])
            .await
            .unwrap_err();
        assert!(
            format!("{err:#}").contains("namespace"),
            "expected a namespace error, got: {err:#}"
        );
        // The offending table must not survive the rolled-back transaction.
        assert!(!has_table(&db, "rogue").await);
    }
}
