//! Admin GET/PUT for the settings row. Mounted by the host at
//! `/api/apps/linear/settings` behind the console session gate.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono_tz::Tz;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::{Recipients, Settings};

/// Wire form: lists/enums are friendly JSON (array of ints, string tags) for the
/// dashboard, decoded to [`Settings`] on save.
#[derive(Serialize, Deserialize)]
struct SettingsWire {
    subscriber_on_comment: bool,
    subscriber_on_status_change: bool,
    subscriber_on_any_update: bool,
    reminders_enabled: bool,
    /// `"assignee"` | `"assignee_and_subscribers"`.
    reminder_recipients: String,
    reminder_lead_days: Vec<i64>,
    reminder_overdue_max_days: i64,
    reminder_check_interval_hours: u64,
    /// IANA timezone, e.g. `"UTC"` or `"Asia/Shanghai"`.
    reminder_timezone: String,
}

impl From<&Settings> for SettingsWire {
    fn from(s: &Settings) -> Self {
        Self {
            subscriber_on_comment: s.subscriber_on_comment,
            subscriber_on_status_change: s.subscriber_on_status_change,
            subscriber_on_any_update: s.subscriber_on_any_update,
            reminders_enabled: s.reminders_enabled,
            reminder_recipients: s.reminder_recipients.as_str().to_string(),
            reminder_lead_days: s.reminder_lead_days.clone(),
            reminder_overdue_max_days: s.reminder_overdue_max_days,
            reminder_check_interval_hours: s.reminder_check_interval_hours,
            reminder_timezone: s.reminder_timezone.name().to_string(),
        }
    }
}

pub fn router(db: DatabaseConnection) -> Router {
    Router::new()
        .route("/settings", get(get_settings).put(put_settings))
        .with_state(db)
}

async fn get_settings(State(db): State<DatabaseConnection>) -> Json<SettingsWire> {
    Json((&super::load(&db).await).into())
}

async fn put_settings(
    State(db): State<DatabaseConnection>,
    Json(body): Json<SettingsWire>,
) -> Result<Json<SettingsWire>, (StatusCode, String)> {
    let tz: Tz = body
        .reminder_timezone
        .parse()
        .map_err(|_| bad(format!("unknown timezone '{}'", body.reminder_timezone)))?;
    if !matches!(
        body.reminder_recipients.as_str(),
        "assignee" | "assignee_and_subscribers"
    ) {
        return Err(bad(format!(
            "invalid reminder_recipients '{}'",
            body.reminder_recipients
        )));
    }
    if body.reminder_check_interval_hours == 0 {
        return Err(bad("reminder_check_interval_hours must be ≥ 1"));
    }

    let settings = Settings {
        subscriber_on_comment: body.subscriber_on_comment,
        subscriber_on_status_change: body.subscriber_on_status_change,
        subscriber_on_any_update: body.subscriber_on_any_update,
        reminders_enabled: body.reminders_enabled,
        reminder_recipients: Recipients::parse(&body.reminder_recipients),
        reminder_lead_days: body
            .reminder_lead_days
            .into_iter()
            .filter(|&d| d >= 0)
            .collect(),
        reminder_overdue_max_days: body.reminder_overdue_max_days.max(0),
        reminder_check_interval_hours: body.reminder_check_interval_hours,
        reminder_timezone: tz,
    };

    super::save(&db, &settings)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json((&settings).into()))
}

fn bad(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.into())
}
