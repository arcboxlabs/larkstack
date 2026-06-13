use std::sync::Arc;

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use larkstack_core::ActionEnvelope;
use larkoapi::LarkBotClient;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{info, warn};

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

pub async fn handle_actions(
    cfg: StandupConfig,
    bot: Arc<LarkBotClient>,
    mut rx: mpsc::UnboundedReceiver<ActionEnvelope>,
) {
    while let Some(env) = rx.recv().await {
        let result = match env.name.as_str() {
            "announce" => with_date(env.params, tomorrow(), |d| {
                let (cfg, bot) = (cfg.clone(), Arc::clone(&bot));
                async move { flow::announce(&cfg, &bot, d).await }
            })
            .await
            .map(|d| format!("announce {d}")),
            "ensure" => with_date(env.params, tomorrow(), |d| {
                let (cfg, bot) = (cfg.clone(), Arc::clone(&bot));
                async move { flow::ensure(&cfg, &bot, d).await }
            })
            .await
            .map(|d| format!("ensure {d}")),
            "remind" => with_date(env.params, today(), |d| {
                let (cfg, bot) = (cfg.clone(), Arc::clone(&bot));
                async move { flow::remind(&cfg, &bot, d, false).await }
            })
            .await
            .map(|d| format!("remind {d}")),
            "urgent" => with_date(env.params, today(), |d| {
                let (cfg, bot) = (cfg.clone(), Arc::clone(&bot));
                async move { flow::remind(&cfg, &bot, d, true).await }
            })
            .await
            .map(|d| format!("urgent {d}")),
            "check" => with_date(env.params, today(), |d| {
                let (cfg, bot) = (cfg.clone(), Arc::clone(&bot));
                async move { flow::check(&cfg, &bot, d).await }
            })
            .await
            .map(|d| format!("check {d}")),
            "urgent-user" => match serde_json::from_value::<UrgentUserParams>(env.params) {
                Ok(p) => {
                    let d = resolve_date(p.date.as_deref(), today());
                    flow::urgent_one(&cfg, &bot, d, &p.open_id)
                        .await
                        .map(|_| format!("urgent-user {} {d}", p.open_id))
                }
                Err(e) => Err(format!("invalid params (need open_id): {e}")),
            },
            other => {
                warn!(action = other, "unknown action");
                continue;
            }
        };
        match result {
            Ok(msg) => info!(action = %env.name, "ok: {msg}"),
            Err(e) => warn!(action = %env.name, "failed: {e}"),
        }
    }
}

async fn with_date<F, Fut>(
    params: serde_json::Value,
    default: NaiveDate,
    run: F,
) -> Result<NaiveDate, String>
where
    F: FnOnce(NaiveDate) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let p: DateParams = serde_json::from_value(params).unwrap_or_default();
    let date = resolve_date(p.date.as_deref(), default);
    run(date).await.map(|_| date)
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
