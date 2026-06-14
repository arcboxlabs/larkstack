use std::sync::Arc;

use anyhow::Context;
use larkoapi::{LarkBotClient, WsEventHandler, ws};
use larkstack_core::ControlHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::{AppConfig, SttProvider};
use crate::events::RecordingReadyHandler;
use crate::pipeline::Pipeline;
use crate::stt;

/// Default parallel transcription concurrency when not overridden via env.
const DEFAULT_CONCURRENCY: usize = 2;

/// Build a [`Pipeline`] from a typed config — synchronous, cheap, returns
/// immediately. Split out of [`run`] so callers can hand the pipeline to the
/// action handler before the WS loop blocks.
pub fn build_pipeline(cfg: &AppConfig) -> anyhow::Result<Arc<Pipeline>> {
    if cfg.lark.app_id.is_empty() || cfg.lark.app_secret.is_empty() {
        anyhow::bail!("LARK_APP_ID / LARK_APP_SECRET required");
    }
    let http = reqwest::Client::new();
    let bot = Arc::new(LarkBotClient::new(
        cfg.lark.app_id.clone(),
        cfg.lark.app_secret.clone(),
        cfg.lark.base_url.clone(),
        http.clone(),
    ));
    let stt_backend =
        stt::build(&cfg.stt).with_context(|| format!("stt {:?}", cfg.stt.provider))?;
    info!(
        provider = ?cfg.stt.provider,
        model = %provider_model(cfg),
        "minutes: stt backend ready ({})",
        stt_backend.name()
    );
    Ok(Arc::new(Pipeline {
        client: bot,
        stt: stt_backend,
        stt_cfg: cfg.stt.clone(),
        digest_cfg: cfg.digest.clone(),
        http,
    }))
}

/// Run the Lark VC WS listener until `cancel` fires. Shared by the embedded App
/// instance and the standalone binary.
pub async fn serve_ws(
    cfg: &AppConfig,
    pipeline: Arc<Pipeline>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let handler: Arc<dyn WsEventHandler> =
        Arc::new(RecordingReadyHandler::new(pipeline, DEFAULT_CONCURRENCY));
    let http_ws = reqwest::Client::new();
    info!(
        concurrency = DEFAULT_CONCURRENCY,
        "minutes: starting WS long connection"
    );
    tokio::select! {
        _ = ws::run_ws_client(
            &cfg.lark.base_url,
            &cfg.lark.app_id,
            &cfg.lark.app_secret,
            handler,
            http_ws,
        ) => {}
        _ = cancel.cancelled() => info!("minutes: WS shutting down"),
    }
    Ok(())
}

/// Standalone WS runner with its own [`ControlHandle`].
pub async fn run_ws(cfg: &AppConfig, pipeline: Arc<Pipeline>, handle: ControlHandle) {
    handle.running().await;
    let _ = serve_ws(cfg, pipeline, CancellationToken::new()).await;
}

/// Convenience: build + run, for the standalone binary.
pub async fn run(cfg: AppConfig, handle: ControlHandle) -> anyhow::Result<()> {
    let pipeline = build_pipeline(&cfg)?;
    run_ws(&cfg, pipeline, handle).await;
    Ok(())
}

fn provider_model(cfg: &AppConfig) -> String {
    match cfg.stt.provider {
        SttProvider::WhisperApi => cfg.stt.whisper_api_model.clone(),
        SttProvider::WhisperCpp => cfg.stt.whisper_cpp_model.clone(),
    }
}
