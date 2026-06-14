//! Shared date helpers. Every invocation surface — the CLI, console actions, and
//! chat commands — parses the same `today | tomorrow | YYYY-MM-DD` argument, so
//! the logic lives here once. The timezone is admin-configurable (the
//! [`settings`](crate::db::settings) row), so callers pass it in.

use chrono::{Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use tracing::warn;

/// Current date in `tz`.
pub fn today(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}

/// Tomorrow in `tz`.
pub fn tomorrow(tz: Tz) -> NaiveDate {
    today(tz) + Duration::days(1)
}

/// Resolve a date argument: `None`/empty → `default`, the `today`/`tomorrow`
/// keywords (in `tz`), or an explicit `YYYY-MM-DD`. A malformed date logs a
/// warning and falls back to `default`.
pub fn resolve(arg: Option<&str>, default: NaiveDate, tz: Tz) -> NaiveDate {
    match arg {
        None | Some("") => default,
        Some("today") => today(tz),
        Some("tomorrow") => tomorrow(tz),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|e| {
            warn!("bad date {s:?}: {e}; using {default}");
            default
        }),
    }
}
