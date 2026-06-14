//! Due-date reminder cadence — the pure logic deciding *which* reminder (if any)
//! is due for an issue, given how far off its deadline is.
//!
//! The scheduler ([`crate::scheduler`]) supplies `days_until` (computed from the
//! issue's `dueDate` and today-in-timezone) and the admin-tuned cadence; this
//! module decides the single applicable tier. The DB ([`crate::db::due_reminders`])
//! then dedupes so each tier fires at most once per deadline.

use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;

/// Today's date in `tz`. Separated from [`days_between`] so the diff stays pure
/// and unit-testable (the wall clock is the only impure input).
pub fn today_in(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}

/// Days from `today` until `due_date` (a `YYYY-MM-DD` `TimelessDate`). Negative
/// once overdue. `None` if the date can't be parsed.
pub fn days_between(due_date: &str, today: NaiveDate) -> Option<i64> {
    let due = NaiveDate::parse_from_str(due_date, "%Y-%m-%d").ok()?;
    Some((due - today).num_days())
}

/// The reminder tier that currently applies to an issue `days_until` its due
/// date, or `None` when no reminder is due.
///
/// Scheduled tiers are the configured `lead_days` (e.g. `[7, 3, 1, 0]`); the
/// applicable one is the *smallest* lead `≥ days_until`, so the deadline nearing
/// fires one nudge per threshold — and an issue first seen late fires only its
/// current threshold, never a backlog blast of every earlier tier. Once overdue
/// (`days_until < 0`) each day is its own tier (`-1, -2, …`) up to `overdue_max`
/// days, giving the "daily while overdue (capped)" behavior.
pub fn current_tier(days_until: i64, lead_days: &[i64], overdue_max: i64) -> Option<i32> {
    if days_until >= 0 {
        lead_days
            .iter()
            .copied()
            .filter(|&l| l >= 0 && l >= days_until)
            .min()
            .map(|l| l as i32)
    } else if overdue_max > 0 && -days_until <= overdue_max {
        Some(days_until as i32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEADS: [i64; 4] = [7, 3, 1, 0];

    #[test]
    fn scheduled_tiers_fire_one_threshold_at_a_time() {
        // Far out: no reminder yet.
        assert_eq!(current_tier(10, &LEADS, 7), None);
        // Within a week → the 7-day nudge; it stays current until the next
        // threshold is crossed.
        assert_eq!(current_tier(7, &LEADS, 7), Some(7));
        assert_eq!(current_tier(5, &LEADS, 7), Some(7));
        assert_eq!(current_tier(4, &LEADS, 7), Some(7));
        // Crossing 3 days → the 3-day nudge (also when first seen at 2 days).
        assert_eq!(current_tier(3, &LEADS, 7), Some(3));
        assert_eq!(current_tier(2, &LEADS, 7), Some(3));
        assert_eq!(current_tier(1, &LEADS, 7), Some(1));
        // Day-of.
        assert_eq!(current_tier(0, &LEADS, 7), Some(0));
    }

    #[test]
    fn overdue_tiers_are_one_per_day_up_to_the_cap() {
        assert_eq!(current_tier(-1, &LEADS, 7), Some(-1));
        assert_eq!(current_tier(-7, &LEADS, 7), Some(-7));
        // Past the cap → stop reminding.
        assert_eq!(current_tier(-8, &LEADS, 7), None);
        // Cap of 0 disables overdue reminders entirely.
        assert_eq!(current_tier(-1, &LEADS, 0), None);
    }

    #[test]
    fn day_of_only_without_a_zero_lead_uses_nearest_lead() {
        // No 0 in the leads: at day-of there's no lead ≥ 0 except the positives,
        // so the smallest positive lead (1) still applies.
        let leads = [7, 3, 1];
        assert_eq!(current_tier(0, &leads, 7), Some(1));
        assert_eq!(current_tier(1, &leads, 7), Some(1));
    }

    #[test]
    fn days_between_parses_and_diffs() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        assert_eq!(days_between("2026-06-21", today), Some(7));
        assert_eq!(days_between("2026-06-14", today), Some(0));
        assert_eq!(days_between("2026-06-10", today), Some(-4));
        assert_eq!(days_between("not-a-date", today), None);
    }
}
