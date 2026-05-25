use std::process::ExitCode;
use std::sync::Arc;

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use chrono_tz::Asia::Shanghai;
use larkoapi::{LarkBotClient, WsEventHandler, ws};
use tracing::{error, info, warn};

use standup_bot::{AppConfig, commands::CommandBot, flow, scheduler};

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
    if cfg.lark.app_id.is_empty() || cfg.lark.app_secret.is_empty() {
        error!("LARK_APP_ID / LARK_APP_SECRET required");
        return ExitCode::FAILURE;
    }

    let http = reqwest::Client::new();
    let bot = Arc::new(LarkBotClient::new(
        cfg.lark.app_id.clone(),
        cfg.lark.app_secret.clone(),
        cfg.lark.base_url.clone(),
        http,
    ));

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("run");
    let date_arg = args.get(1).map(String::as_str);

    let today = Utc::now().with_timezone(&Shanghai).date_naive();
    let tomorrow = today + ChronoDuration::days(1);

    let result = match cmd {
        "run" => {
            // Resolve bot open_id once so the command handler can tell whether
            // a group message @-mentioned the bot.
            match bot.bot_open_id().await {
                Ok(bot_open_id) => {
                    info!("standup: bot open_id = {bot_open_id}");
                    let handler: std::sync::Arc<dyn WsEventHandler> =
                        std::sync::Arc::new(CommandBot {
                            cfg: std::sync::Arc::new(cfg.standup.clone()),
                            client: std::sync::Arc::clone(&bot),
                            bot_open_id,
                        });
                    let base_url = cfg.lark.base_url.clone();
                    let app_id = cfg.lark.app_id.clone();
                    let app_secret = cfg.lark.app_secret.clone();
                    let http_ws = reqwest::Client::new();
                    tokio::spawn(async move {
                        ws::run_ws_client(&base_url, &app_id, &app_secret, handler, http_ws).await;
                    });
                }
                Err(e) => warn!("standup: bot_open_id lookup failed ({e}); command bot disabled"),
            }
            scheduler::run(cfg.standup, bot).await;
            return ExitCode::SUCCESS;
        }
        "announce" => flow::announce(&cfg.standup, &bot, resolve_date(date_arg, tomorrow)).await,
        "ensure" => flow::ensure(&cfg.standup, &bot, resolve_date(date_arg, tomorrow)).await,
        "remind" => flow::remind(&cfg.standup, &bot, resolve_date(date_arg, today), false).await,
        "urgent" => flow::remind(&cfg.standup, &bot, resolve_date(date_arg, today), true).await,
        "urgent-user" => {
            let Some(uid) = args.get(1) else {
                eprintln!("usage: standup-bot urgent-user <open_id> [date]");
                return ExitCode::from(2);
            };
            let date = args.get(2).map(String::as_str);
            flow::urgent_one(&cfg.standup, &bot, resolve_date(date, today), uid).await
        }
        "check" => flow::check(&cfg.standup, &bot, resolve_date(date_arg, today)).await,
        "help" | "--help" | "-h" => {
            print_help();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{cmd} failed: {e}");
            ExitCode::FAILURE
        }
    }
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
    eprintln!();
    eprintln!("env vars (required):");
    eprintln!("  LARK_APP_ID, LARK_APP_SECRET");
    eprintln!("  STANDUP_CHAT_ID, STANDUP_FOLDER_TOKEN");
    eprintln!("  STANDUP_ENABLED=true  (only required for scheduler mode)");
    eprintln!("  LARK_BASE_URL  (default https://open.larksuite.com)");
}
