use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use larkoapi::{LarkBotClient, WsEventHandler, ws};
use tracing::{error, info};

use meeting_digest::events::RecordingReadyHandler;
use meeting_digest::pipeline::Pipeline;
use meeting_digest::stt;
use meeting_digest::{AppConfig, SttProvider};

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
    Run {
        /// Max parallel transcriptions.
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
    },
    /// Process one meeting by ID (backfill / manual test).
    Process {
        /// VC meeting ID.
        meeting_id: String,
        /// Override recipient for the digest DM.
        #[arg(long)]
        owner: Option<String>,
        /// Skip the VC recording lookup and use this URL directly.
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
    if cfg.lark.app_id.is_empty() || cfg.lark.app_secret.is_empty() {
        error!("LARK_APP_ID / LARK_APP_SECRET required");
        return ExitCode::FAILURE;
    }

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
    info!(
        provider = ?cfg.stt.provider,
        model = ?provider_model(&cfg),
        "digest: stt backend ready ({})",
        stt_backend.name()
    );

    let pipeline = Arc::new(Pipeline {
        client: Arc::clone(&bot),
        stt: stt_backend,
        stt_cfg: cfg.stt.clone(),
        digest_cfg: cfg.digest.clone(),
        http,
    });

    match cli.command.unwrap_or(Command::Run { concurrency: 2 }) {
        Command::Run { concurrency } => run_ws(cfg, pipeline, concurrency).await,
        Command::Process {
            meeting_id,
            owner,
            url,
        } => {
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
    }
}

async fn run_ws(cfg: AppConfig, pipeline: Arc<Pipeline>, concurrency: usize) -> ExitCode {
    let handler: Arc<dyn WsEventHandler> =
        Arc::new(RecordingReadyHandler::new(pipeline, concurrency));
    let http_ws = reqwest::Client::new();
    info!("digest: starting WS long connection (concurrency={concurrency})");
    ws::run_ws_client(
        &cfg.lark.base_url,
        &cfg.lark.app_id,
        &cfg.lark.app_secret,
        handler,
        http_ws,
    )
    .await;
    ExitCode::SUCCESS
}

fn provider_model(cfg: &AppConfig) -> String {
    match cfg.stt.provider {
        SttProvider::WhisperApi => cfg.stt.whisper_api_model.clone(),
        SttProvider::WhisperCpp => cfg.stt.whisper_cpp_model.clone(),
    }
}
