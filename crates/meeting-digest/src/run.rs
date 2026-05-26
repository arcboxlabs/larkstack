use std::sync::Arc;

use anyhow::Context;
use control::ControlHandle;
use larkoapi::{LarkBotClient, WsEventHandler, ws};
use tracing::info;

use crate::config::{AppConfig, SttProvider};
use crate::events::RecordingReadyHandler;
use crate::pipeline::Pipeline;
use crate::stt;

/// Default parallel transcription concurrency when not overridden via env.
const DEFAULT_CONCURRENCY: usize = 2;

/// Build a [`Pipeline`] from a typed config — synchronous, cheap, returns
/// immediately. Split out of [`run`] so callers (the console) can hand the
/// pipeline to other tasks (e.g. the action handler) before the WS loop
/// blocks forever.
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
        "meeting-digest: stt backend ready ({})",
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

/// Run the Lark VC WS listener forever. Returns only when the WS task exits
/// (typically when the surrounding tokio task is aborted by the supervisor).
pub async fn run_ws(cfg: &AppConfig, pipeline: Arc<Pipeline>, handle: ControlHandle) {
    let handler: Arc<dyn WsEventHandler> =
        Arc::new(RecordingReadyHandler::new(pipeline, DEFAULT_CONCURRENCY));
    let http_ws = reqwest::Client::new();
    info!(
        concurrency = DEFAULT_CONCURRENCY,
        "meeting-digest: starting WS long connection"
    );
    handle.running().await;
    ws::run_ws_client(
        &cfg.lark.base_url,
        &cfg.lark.app_id,
        &cfg.lark.app_secret,
        handler,
        http_ws,
    )
    .await;
}

/// Convenience: build + run, for callers (standalone bin) that don't need
/// the pipeline handle.
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
