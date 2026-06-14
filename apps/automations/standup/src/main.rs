use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use larkoapi::LarkBotClient;
use larkstack_core::{ControlPlane, SqliteStateStore, StateStore};
use tracing::error;

use standup::{AppConfig, date, flow, settings};

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

    // Standalone runs without the host, so it opens the per-App state store
    // itself (the host hands one in for the console). Settings live there.
    let data_dir =
        PathBuf::from(std::env::var("CONSOLE_DATA_DIR").unwrap_or_else(|_| "data".to_string()));
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        error!("create data dir: {e}");
        return ExitCode::FAILURE;
    }
    let store: Arc<dyn StateStore> = match SqliteStateStore::open(data_dir.join("state.db")) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!("open state store: {e}");
            return ExitCode::FAILURE;
        }
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("run");
    let date_arg = args.get(1).map(String::as_str);

    let s = settings::load(&store).await;
    let today = date::today(s.timezone);
    let tomorrow = date::tomorrow(s.timezone);

    match cmd {
        "run" => {
            let plane = ControlPlane::new();
            let handle = plane.handle("standup");
            match standup::run(cfg, store, handle).await {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("standup: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        "announce" | "ensure" | "remind" | "urgent" | "check" => {
            let bot = build_bot(&cfg);
            let date = match cmd {
                "announce" | "ensure" => date::resolve(date_arg, tomorrow, s.timezone),
                _ => date::resolve(date_arg, today, s.timezone),
            };
            let result = match cmd {
                "announce" => flow::announce(&cfg.standup, &s, &bot, date).await,
                "ensure" => flow::ensure(&cfg.standup, &s, &bot, date).await,
                "remind" => flow::remind(&cfg.standup, &s, &bot, date, false).await,
                "urgent" => flow::remind(&cfg.standup, &s, &bot, date, true).await,
                "check" => flow::check(&cfg.standup, &s, &bot, date).await,
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
                eprintln!("usage: standup urgent-user <open_id> [date]");
                return ExitCode::from(2);
            };
            let bot = build_bot(&cfg);
            let date = date::resolve(args.get(2).map(String::as_str), today, s.timezone);
            match flow::urgent_one(&cfg.standup, &s, &bot, date, uid).await {
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

fn print_help() {
    eprintln!("standup — Daily Standup reminder for Lark/Feishu");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  standup                      run scheduler (default)");
    eprintln!("  standup run                  same as above");
    eprintln!();
    eprintln!("Manual triggers (one-shot, exit after):");
    eprintln!("  standup ensure   [date]      create doc + share with chat (no chat card)");
    eprintln!("  standup announce [date]      ensure doc + post announcement card to chat");
    eprintln!("  standup remind   [date]      DM everyone still empty");
    eprintln!("  standup urgent   [date]      DM + in-app urgent escalation");
    eprintln!("  standup urgent-user <open_id> [date]");
    eprintln!("                                   urgent one specific user (for testing)");
    eprintln!("  standup check    [date]      list missing fillers (read-only)");
    eprintln!();
    eprintln!("date:  today | tomorrow | YYYY-MM-DD");
    eprintln!("       default is `tomorrow` for ensure/announce, `today` for the rest");
}
