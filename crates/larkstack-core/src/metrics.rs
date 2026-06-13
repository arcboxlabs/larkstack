//! `MetricsSink`: an append-only sink for analytics metrics (counts, gauges).
//! SQLite-backed today; ClickHouse is a planned opt-in backend for higher
//! volume (it suits the append-only shape — unlike [`StateStore`](crate::StateStore),
//! whose mutable point-updates do not fit a columnar store).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use rusqlite::{Connection, params};

/// A single metric point: a named counter/gauge with string labels. The sink
/// stamps the wall-clock time on write.
#[derive(Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub labels: BTreeMap<String, String>,
}

impl Metric {
    /// A unit increment (`value = 1.0`).
    pub fn count(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: 1.0,
            labels: BTreeMap::new(),
        }
    }

    /// A measured value.
    pub fn gauge(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
            labels: BTreeMap::new(),
        }
    }

    /// Attach a label dimension (chainable).
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

#[async_trait]
pub trait MetricsSink: Send + Sync {
    /// Append one metric point.
    async fn record(&self, metric: Metric) -> anyhow::Result<()>;
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS metrics (
    ts     INTEGER NOT NULL,
    name   TEXT NOT NULL,
    value  REAL NOT NULL,
    labels TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_metrics_name_ts ON metrics(name, ts);
"#;

/// SQLite-backed [`MetricsSink`].
#[derive(Clone)]
pub struct SqliteMetricsSink {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteMetricsSink {
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
impl MetricsSink for SqliteMetricsSink {
    async fn record(&self, metric: Metric) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let labels = serde_json::to_string(&metric.labels)?;
            let conn = conn.lock().expect("metrics sink mutex");
            conn.execute(
                "INSERT INTO metrics (ts, name, value, labels) VALUES (?1, ?2, ?3, ?4)",
                params![now_ms(), metric.name, metric.value, labels],
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
