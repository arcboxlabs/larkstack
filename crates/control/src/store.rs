use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use rusqlite::{Connection, params};

use crate::{Event, Level};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id        INTEGER PRIMARY KEY,
    level     TEXT NOT NULL,
    subsystem TEXT,
    target    TEXT NOT NULL,
    message   TEXT NOT NULL,
    fields    TEXT NOT NULL,
    timestamp INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_subsystem ON events(subsystem);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
"#;

/// Rolling retention cap. The store keeps the most recent
/// `RETENTION_CAP` events; older rows are dropped on each insert batch.
const RETENTION_CAP: i64 = 10_000;

/// SQLite-backed event log. All operations move work onto a blocking thread
/// so they can be awaited from axum handlers without blocking the reactor.
#[derive(Clone)]
pub struct EventStore {
    conn: Arc<Mutex<Connection>>,
}

impl EventStore {
    /// Open or create the database file at `path`. The parent directory must
    /// exist; the caller is responsible for creating it.
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

    /// Append a single event. Trims older rows beyond [`RETENTION_CAP`].
    pub async fn persist(&self, event: Event) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock().expect("event store mutex");
            conn.execute(
                "INSERT INTO events (id, level, subsystem, target, message, fields, timestamp) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.id as i64,
                    level_str(event.level),
                    event.subsystem,
                    event.target,
                    event.message,
                    serde_json::to_string(&event.fields)?,
                    timestamp_ms(event.timestamp),
                ],
            )?;
            // Cheap retention check — usually a no-op once steady-state.
            conn.execute(
                "DELETE FROM events WHERE id <= (SELECT MAX(id) FROM events) - ?1",
                params![RETENTION_CAP],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    /// Events with `id > since`, oldest first, capped at `limit`.
    pub async fn since(&self, since: u64, limit: usize) -> anyhow::Result<Vec<Event>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Event>> {
            let conn = conn.lock().expect("event store mutex");
            let mut stmt = conn.prepare(
                "SELECT id, level, subsystem, target, message, fields, timestamp \
                 FROM events WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![since as i64, limit as i64], row_to_event)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        })
        .await?
    }

    /// Last `limit` events, oldest first.
    pub async fn recent(&self, limit: usize) -> anyhow::Result<Vec<Event>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Event>> {
            let conn = conn.lock().expect("event store mutex");
            let mut stmt = conn.prepare(
                "SELECT id, level, subsystem, target, message, fields, timestamp \
                 FROM events ORDER BY id DESC LIMIT ?1",
            )?;
            let mut rows = stmt
                .query_map(params![limit as i64], row_to_event)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.reverse();
            Ok(rows)
        })
        .await?
    }

    /// Highest event id in the store, or `None` if empty. Lets the console
    /// reset the in-memory id counter on startup so new events stay ordered.
    pub async fn max_id(&self) -> anyhow::Result<Option<u64>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<Option<u64>> {
            let conn = conn.lock().expect("event store mutex");
            let max: Option<i64> = conn
                .query_row("SELECT MAX(id) FROM events", [], |row| row.get(0))
                .ok();
            Ok(max.map(|n| n as u64))
        })
        .await?
    }
}

fn level_str(l: Level) -> &'static str {
    match l {
        Level::Trace => "trace",
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

fn parse_level(s: &str) -> Level {
    match s {
        "trace" => Level::Trace,
        "debug" => Level::Debug,
        "warn" => Level::Warn,
        "error" => Level::Error,
        _ => Level::Info,
    }
}

fn timestamp_ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn from_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let id: i64 = row.get(0)?;
    let level: String = row.get(1)?;
    let subsystem: Option<String> = row.get(2)?;
    let target: String = row.get(3)?;
    let message: String = row.get(4)?;
    let fields: String = row.get(5)?;
    let timestamp: i64 = row.get(6)?;
    Ok(Event {
        id: id as u64,
        level: parse_level(&level),
        subsystem,
        target,
        message,
        fields: serde_json::from_str(&fields).unwrap_or_default(),
        timestamp: from_ms(timestamp),
    })
}
