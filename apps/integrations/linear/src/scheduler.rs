//! Background due-date reminder loop. Polls Linear for due-dated issues and DMs
//! the responsible user(s) on the admin-tuned cadence ([`crate::domain::reminders`]),
//! deduping per deadline via [`crate::db::due_reminders`].
//!
//! Runs alongside the webhook server (see [`crate::app`]); both honor the same
//! `CancellationToken`. No-op while reminders are disabled or `LINEAR_API_KEY`
//! is unset (subscriber/assignee emails come from the GraphQL API).

use std::sync::Arc;
use std::time::Duration;

use chrono::Duration as Days;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::AppState;
use crate::db::settings::{self, Settings};
use crate::db::{due_reminders, user_map};
use crate::domain::reminders;
use crate::lark::{cards, notify};
use crate::source::api::DueIssue;

/// Loop until `cancel` fires, running a reminder pass each tick and sleeping for
/// the (live-reloaded) check interval in between.
pub async fn run_scheduler(state: Arc<AppState>, cancel: CancellationToken) -> anyhow::Result<()> {
    let mut warned_no_client = false;
    loop {
        let settings = settings::load(&state.db).await;

        if settings.reminders_enabled {
            if state.linear_client.is_some() {
                if let Err(e) = run_pass(&state, &settings).await {
                    warn!("reminder pass failed: {e}");
                }
            } else if !warned_no_client {
                warn!("due-date reminders enabled but LINEAR_API_KEY is unset – skipping");
                warned_no_client = true;
            }
        }

        let interval = Duration::from_secs(settings.reminder_check_interval_hours * 3600);
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {}
        }
    }
    Ok(())
}

/// One scan: fetch due-dated issues in the cadence window and fire any tier not
/// already sent.
async fn run_pass(state: &AppState, settings: &Settings) -> anyhow::Result<()> {
    let client = state
        .linear_client
        .as_ref()
        .expect("caller checked linear_client is set");

    let today = reminders::today_in(settings.reminder_timezone);
    let max_lead = settings
        .reminder_lead_days
        .iter()
        .copied()
        .max()
        .unwrap_or(0);
    let gte = (today - Days::days(settings.reminder_overdue_max_days))
        .format("%Y-%m-%d")
        .to_string();
    let lte = (today + Days::days(max_lead))
        .format("%Y-%m-%d")
        .to_string();

    let issues = client
        .fetch_issues_due_soon(&gte, &lte)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    info!(
        "reminder pass: {} due-dated issue(s) in [{gte}, {lte}]",
        issues.len()
    );

    for issue in issues {
        let Some(days_until) = reminders::days_between(&issue.due_date, today) else {
            continue;
        };
        let Some(tier) = reminders::current_tier(
            days_until,
            &settings.reminder_lead_days,
            settings.reminder_overdue_max_days,
        ) else {
            continue;
        };
        if due_reminders::already_sent(&state.db, &issue.id, &issue.due_date, tier).await {
            continue;
        }

        send_reminder(state, settings, &issue, days_until).await;
        if let Err(e) = due_reminders::record(&state.db, &issue.id, &issue.due_date, tier).await {
            warn!(
                "failed to record reminder {}@{} tier {tier}: {e}",
                issue.identifier, issue.due_date
            );
        }
    }
    Ok(())
}

/// DM the reminder card to the assignee (and subscribers when configured),
/// resolving each Linear email to its Lark address and deduping recipients.
async fn send_reminder(state: &AppState, settings: &Settings, issue: &DueIssue, days_until: i64) {
    let card = cards::reminder_dm(issue, days_until);

    let mut linear_emails = Vec::new();
    if let Some(assignee) = &issue.assignee
        && !assignee.email.is_empty()
    {
        linear_emails.push(assignee.email.clone());
    }
    if settings.reminder_recipients.includes_subscribers() {
        for s in &issue.subscribers {
            if s.active && !s.email.is_empty() {
                linear_emails.push(s.email.clone());
            }
        }
    }
    linear_emails.sort();
    linear_emails.dedup();

    let mut lark_emails = Vec::with_capacity(linear_emails.len());
    for email in &linear_emails {
        lark_emails.push(user_map::resolve_lark_email(&state.db, email).await);
    }
    notify::dm_many(state, &lark_emails, &card).await;
}
