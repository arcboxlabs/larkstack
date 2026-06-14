use std::sync::Arc;

use anyhow::Context;
use chrono::NaiveDate;
use larkoapi::LarkBotClient;
use serde::Deserialize;
use serde_json::Value;

use crate::config::StandupConfig;
use crate::date;
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
            let d = date_param(&params, date::tomorrow());
            flow::announce(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("announce {d}"))
        }
        "ensure" => {
            let d = date_param(&params, date::tomorrow());
            flow::ensure(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("ensure {d}"))
        }
        "remind" => {
            let d = date_param(&params, date::today());
            flow::remind(cfg, bot, d, false).await.map_err(into_err)?;
            Ok(format!("remind {d}"))
        }
        "urgent" => {
            let d = date_param(&params, date::today());
            flow::remind(cfg, bot, d, true).await.map_err(into_err)?;
            Ok(format!("urgent {d}"))
        }
        "check" => {
            let d = date_param(&params, date::today());
            flow::check(cfg, bot, d).await.map_err(into_err)?;
            Ok(format!("check {d}"))
        }
        "urgent-user" => {
            let p: UrgentUserParams =
                serde_json::from_value(params).context("invalid params (need open_id)")?;
            let d = date::resolve(p.date.as_deref(), date::today());
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
    date::resolve(p.date.as_deref(), default)
}
