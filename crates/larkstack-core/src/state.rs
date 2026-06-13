//! `StateStore`: a namespaced, mutable key→value store for App state that must
//! survive restarts — e.g. tracking a posted card's message id so a later event
//! updates it in place instead of reposting. SQLite-backed today; Redis is a
//! possible future backend (hence the trait).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};

#[async_trait]
pub trait StateStore: Send + Sync {
    /// Fetch the value for `(namespace, key)`, or `None` if absent.
    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<String>>;
    /// Insert or overwrite the value for `(namespace, key)`.
    async fn set(&self, namespace: &str, key: &str, value: &str) -> anyhow::Result<()>;
    /// Remove `(namespace, key)` if present.
    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<()>;
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS app_state (
    namespace  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (namespace, key)
);
"#;

/// SQLite-backed [`StateStore`]. Work runs on a blocking thread so it can be
/// awaited from async App code without stalling the reactor.
#[derive(Clone)]
pub struct SqliteStateStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStateStore {
    /// Open or create the database at `path`; the parent directory must exist.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("open sqlite at {}", path.as_ref().display()))?;
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl StateStore for SqliteStateStore {
    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.clone();
        let (ns, k) = (namespace.to_string(), key.to_string());
        tokio::task::spawn_blocking(move || -> anyhow::Result<Option<String>> {
            let conn = conn.lock().expect("state store mutex");
            let value = conn
                .query_row(
                    "SELECT value FROM app_state WHERE namespace = ?1 AND key = ?2",
                    params![ns, k],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(value)
        })
        .await?
    }

    async fn set(&self, namespace: &str, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let (ns, k, v) = (namespace.to_string(), key.to_string(), value.to_string());
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock().expect("state store mutex");
            conn.execute(
                "INSERT INTO app_state (namespace, key, value, updated_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(namespace, key) DO UPDATE SET value = ?3, updated_at = ?4",
                params![ns, k, v, now_ms()],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let (ns, k) = (namespace.to_string(), key.to_string());
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock().expect("state store mutex");
            conn.execute(
                "DELETE FROM app_state WHERE namespace = ?1 AND key = ?2",
                params![ns, k],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
