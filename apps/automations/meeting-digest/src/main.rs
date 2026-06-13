use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use larkoapi::LarkBotClient;
use larkstack_core::ControlPlane;
use tracing::error;

use meeting_digest::AppConfig;
use meeting_digest::pipeline::Pipeline;
use meeting_digest::stt;

/// Auto-transcribe Lark recorded meetings and post digest cards.
///
/// Required env: LARK_APP_ID, LARK_APP_SECRET, DIGEST_FOLDER_TOKEN.
/// STT defaults to whisper_api (needs STT_WHISPER_API_KEY); use
/// STT_PROVIDER=whisper_cpp with STT_WHISPER_CPP_MODEL for local.
#[derive(Debug, Parser)]
#[command(name = "meeting-digest", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the Lark WebSocket listener and digest on `recording_ready_v1`.
    Run,
    /// Process one meeting by ID (backfill / manual test).
    Process {
        meeting_id: String,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        url: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = match AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!("load config: {e}");
            return ExitCode::FAILURE;
        }
    };

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            let plane = ControlPlane::new();
            let handle = plane.handle("meeting-digest");
            match meeting_digest::run(cfg, handle).await {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("meeting-digest: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Process {
            meeting_id,
            owner,
            url,
        } => process_one(cfg, meeting_id, owner, url).await,
    }
}

async fn process_one(
    cfg: AppConfig,
    meeting_id: String,
    owner: Option<String>,
    url: Option<String>,
) -> ExitCode {
    let http = reqwest::Client::new();
    let bot = Arc::new(LarkBotClient::new(
        cfg.lark.app_id.clone(),
        cfg.lark.app_secret.clone(),
        cfg.lark.base_url.clone(),
        http.clone(),
    ));
    let stt_backend = match stt::build(&cfg.stt) {
        Ok(s) => s,
        Err(e) => {
            error!("build stt ({:?}): {e}", cfg.stt.provider);
            return ExitCode::FAILURE;
        }
    };
    let pipeline = Pipeline {
        client: bot,
        stt: stt_backend,
        stt_cfg: cfg.stt.clone(),
        digest_cfg: cfg.digest.clone(),
        http,
    };
    match pipeline
        .process_meeting(&meeting_id, owner.as_deref(), url.as_deref())
        .await
    {
        Ok(out) => {
            println!(
                "ok: meeting={} doc={} segments={}",
                out.meeting_id, out.doc_url, out.segments
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!("process {meeting_id} failed: {e}");
            ExitCode::FAILURE
        }
    }
}
