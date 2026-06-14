use std::sync::Arc;

use anyhow::Context;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use larkoapi::LarkBotClient;
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use crate::config::StandupConfig;
use crate::flow;

#[derive(Default, Deserialize)]
struct DateParams {
    #[serde(default)]
    date: Option<String>,
}

#[derive(Deserialize)]
struct UrgentUserParams {
    open_id: String,
    #[serde(default)]
    date: Option<String>,
}

/// Handle one action, returning a human-readable result.
pub async fn handle(
    cfg: &StandupConfig,
    bot: &Arc<LarkBotClient>,
    action: &str,
    params: Value,
) -> anyhow::Result<String> {
    match action {
        "announce" => {
            let d = date_param(&params, tomorrow());
            flow::announce(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("announce {d}"))
        }
        "ensure" => {
            let d = date_param(&params, tomorrow());
            flow::ensure(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("ensure {d}"))
        }
        "remind" => {
            let d = date_param(&params, today());
            flow::remind(cfg, bot, d, false).await.map_err(into_err)?;
            Ok(format!("remind {d}"))
        }
        "urgent" => {
            let d = date_param(&params, today());
            flow::remind(cfg, bot, d, true).await.map_err(into_err)?;
            Ok(format!("urgent {d}"))
        }
        "check" => {
            let d = date_param(&params, today());
            flow::check(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("check {d}"))
        }
        "urgent-user" => {
            let p: UrgentUserParams =
                serde_json::from_value(params).context("invalid params (need open_id)")?;
            let d = resolve_date(p.date.as_deref(), today());
            flow::urgent_one(cfg, bot, d, &p.open_id)
                .await
                .map_err(into_err)?;
            Ok(format!("urgent-user {} {d}", p.open_id))
        }
        other => anyhow::bail!("unknown action '{other}'"),
    }
}

fn into_err(e: String) -> anyhow::Error {
    anyhow::anyhow!(e)
}

fn date_param(params: &Value, default: NaiveDate) -> NaiveDate {
    let p: DateParams = serde_json::from_value(params.clone()).unwrap_or_default();
    resolve_date(p.date.as_deref(), default)
}

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Shanghai).date_naive()
}

fn tomorrow() -> NaiveDate {
    today() + ChronoDuration::days(1)
}

fn resolve_date(arg: Option<&str>, default: NaiveDate) -> NaiveDate {
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
