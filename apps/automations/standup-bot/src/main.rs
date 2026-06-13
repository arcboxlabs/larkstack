use std::process::ExitCode;
use std::sync::Arc;

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use larkoapi::LarkBotClient;
use larkstack_core::ControlPlane;
use tracing::error;

use standup_bot::{AppConfig, flow};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = match AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!("load config: {e}");
            return ExitCode::FAILURE;
        }
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("run");
    let date_arg = args.get(1).map(String::as_str);

    let today = Utc::now().with_timezone(&Shanghai).date_naive();
    let tomorrow = today + ChronoDuration::days(1);

    match cmd {
        "run" => {
            let plane = ControlPlane::new();
            let handle = plane.handle("standup-bot");
            match standup_bot::run(cfg, handle).await {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("standup-bot: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        "announce" | "ensure" | "remind" | "urgent" | "check" => {
            let bot = build_bot(&cfg);
            let date = match cmd {
                "announce" | "ensure" => resolve_date(date_arg, tomorrow),
                _ => resolve_date(date_arg, today),
            };
            let result = match cmd {
                "announce" => flow::announce(&cfg.standup, &bot, date).await,
                "ensure" => flow::ensure(&cfg.standup, &bot, date).await,
                "remind" => flow::remind(&cfg.standup, &bot, date, false).await,
                "urgent" => flow::remind(&cfg.standup, &bot, date, true).await,
                "check" => flow::check(&cfg.standup, &bot, date).await,
                _ => unreachable!(),
            };
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("{cmd} failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "urgent-user" => {
            let Some(uid) = args.get(1) else {
                eprintln!("usage: standup-bot urgent-user <open_id> [date]");
                return ExitCode::from(2);
            };
            let bot = build_bot(&cfg);
            let date = resolve_date(args.get(2).map(String::as_str), today);
            match flow::urgent_one(&cfg.standup, &bot, date, uid).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("urgent-user failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn build_bot(cfg: &AppConfig) -> Arc<LarkBotClient> {
    let http = reqwest::Client::new();
    Arc::new(LarkBotClient::new(
        cfg.lark.app_id.clone(),
        cfg.lark.app_secret.clone(),
        cfg.lark.base_url.clone(),
        http,
    ))
}

fn resolve_date(arg: Option<&str>, default: NaiveDate) -> NaiveDate {
    match arg {
        None => default,
        Some("today") => Utc::now().with_timezone(&Shanghai).date_naive(),
        Some("tomorrow") => {
            Utc::now().with_timezone(&Shanghai).date_naive() + ChronoDuration::days(1)
        }
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|e| {
            eprintln!("bad date {s:?}: {e} — using default {default}");
            default
        }),
    }
}

fn print_help() {
    eprintln!("standup-bot — Daily Standup reminder for Lark/Feishu");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  standup-bot                      run scheduler (default)");
    eprintln!("  standup-bot run                  same as above");
    eprintln!();
    eprintln!("Manual triggers (one-shot, exit after):");
    eprintln!("  standup-bot ensure   [date]      create doc + share with chat (no chat card)");
    eprintln!("  standup-bot announce [date]      ensure doc + post announcement card to chat");
    eprintln!("  standup-bot remind   [date]      DM everyone still empty");
    eprintln!("  standup-bot urgent   [date]      DM + in-app urgent escalation");
    eprintln!("  standup-bot urgent-user <open_id> [date]");
    eprintln!("                                   urgent one specific user (for testing)");
    eprintln!("  standup-bot check    [date]      list missing fillers (read-only)");
    eprintln!();
    eprintln!("date:  today | tomorrow | YYYY-MM-DD");
    eprintln!("       default is `tomorrow` for ensure/announce, `today` for the rest");
}
