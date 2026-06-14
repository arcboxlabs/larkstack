//! Shared date helpers (Asia/Shanghai). Every invocation surface — the CLI,
//! console actions, and chat commands — parses the same `today | tomorrow |
//! YYYY-MM-DD` argument, so the logic lives here once.

use chrono::{Duration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use tracing::warn;

/// Current date in Asia/Shanghai.
pub fn today() -> NaiveDate {
    Utc::now().with_timezone(&Shanghai).date_naive()
}

/// Tomorrow in Asia/Shanghai.
pub fn tomorrow() -> NaiveDate {
    today() + Duration::days(1)
}

/// Resolve a date argument: `None`/empty → `default`, the `today`/`tomorrow`
/// keywords, or an explicit `YYYY-MM-DD`. A malformed date logs a warning and
/// falls back to `default`.
pub fn resolve(arg: Option<&str>, default: NaiveDate) -> NaiveDate {
    match arg {
        None | Some("") => default,
        Some("today") => today(),
        Some("tomorrow") => tomorrow(),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|e| {
            warn!("bad date {s:?}: {e}; using {default}");
            default
        }),
    }
}
