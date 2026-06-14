//! Admin GET/PUT for the settings blob. Mounted by the host at
//! `/api/apps/standup/settings` behind the console session gate.

use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono_tz::Tz;
use larkstack_core::StateStore;

use super::{Settings, SettingsWire, load, save};

type Store = Arc<dyn StateStore>;

pub fn router(store: Store) -> Router {
    Router::new()
        .route("/settings", get(get_settings).put(put_settings))
        .with_state(store)
}

async fn get_settings(State(store): State<Store>) -> Json<SettingsWire> {
    Json((&load(&store).await).into())
}

async fn put_settings(
    State(store): State<Store>,
    Json(body): Json<SettingsWire>,
) -> Result<Json<SettingsWire>, (StatusCode, String)> {
    // Validate before persisting: a bad timezone/time would otherwise silently
    // degrade to a default on the next load.
    let _tz: Tz = body
        .timezone
        .parse()
        .map_err(|_| bad(format!("unknown timezone '{}'", body.timezone)))?;
    for (label, hm) in [
        ("announce_time", &body.announce_time),
        ("remind_evening_time", &body.remind_evening_time),
        ("remind_morning_time", &body.remind_morning_time),
        ("urgent_time", &body.urgent_time),
    ] {
        check_time(label, hm)?;
    }
    if !body.column_widths.iter().any(|&w| w > 0) {
        return Err(bad("column_widths must have at least one positive value"));
    }

    save(&store, &body)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // Echo back the normalized view (e.g. widths filtered) the app will act on.
    Ok(Json((&Settings::from_wire(body)).into()))
}

/// Reject a time that isn't `HH:MM` in valid ranges.
fn check_time(label: &str, s: &str) -> Result<(), (StatusCode, String)> {
    let ok = s.split_once(':').is_some_and(
        |(h, m)| matches!((h.parse::<u32>(), m.parse::<u32>()), (Ok(h), Ok(m)) if h < 24 && m < 60),
    );
    if ok {
        Ok(())
    } else {
        Err(bad(format!("{label} must be HH:MM (got '{s}')")))
    }
}

fn bad(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.into())
}
