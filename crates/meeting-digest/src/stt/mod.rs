//! Speech-to-Text abstraction. Implementations live in sibling modules.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::config::{SttConfig, SttProvider};

pub mod whisper_api;
#[cfg(feature = "whisper-cpp")]
pub mod whisper_cpp;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("stt http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("stt io: {0}")]
    Io(#[from] std::io::Error),
    #[error("stt json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stt api {code}: {message}")]
    Api { code: u16, message: String },
    #[error("stt config: {0}")]
    Config(String),
    #[cfg(feature = "whisper-cpp")]
    #[error("whisper.cpp: {0}")]
    WhisperCpp(String),
}

#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    /// BCP-47 language hint (`zh`, `en`, `ja`, ...). `None` or `"auto"` → auto-detect.
    pub language: Option<String>,
    /// Domain/terminology hint; improves proper-noun accuracy with Whisper.
    pub prompt: Option<String>,
}

impl TranscribeOptions {
    pub fn from_config(cfg: &SttConfig) -> Self {
        let lang = if cfg.language.is_empty() || cfg.language == "auto" {
            None
        } else {
            Some(cfg.language.clone())
        };
        Self {
            language: lang,
            prompt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Transcript {
    pub language: Option<String>,
    pub full_text: String,
    pub segments: Vec<Segment>,
}

#[async_trait]
pub trait SpeechToText: Send + Sync {
    /// Transcribe the audio file at `input`. Implementations may need to
    /// re-encode internally (e.g. whisper.cpp requires 16kHz mono f32 WAV).
    async fn transcribe(
        &self,
        input: &Path,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, SttError>;

    fn name(&self) -> &'static str;
}

pub fn build(cfg: &SttConfig) -> Result<Arc<dyn SpeechToText>, SttError> {
    match cfg.provider {
        SttProvider::WhisperApi => {
            if cfg.whisper_api_key.is_empty() {
                return Err(SttError::Config("STT_WHISPER_API_KEY required".into()));
            }
            Ok(Arc::new(whisper_api::WhisperApi::new(
                cfg.whisper_api_base.clone(),
                cfg.whisper_api_key.clone(),
                cfg.whisper_api_model.clone(),
            )))
        }
        SttProvider::WhisperCpp => {
            #[cfg(feature = "whisper-cpp")]
            {
                if cfg.whisper_cpp_model.is_empty() {
                    return Err(SttError::Config(
                        "STT_WHISPER_CPP_MODEL (path to ggml bin) required".into(),
                    ));
                }
                let threads = if cfg.whisper_cpp_threads == 0 {
                    num_cpus_hint() as u32
                } else {
                    cfg.whisper_cpp_threads
                };
                Ok(Arc::new(whisper_cpp::WhisperCpp::new(
                    cfg.whisper_cpp_model.clone(),
                    threads,
                )?))
            }
            #[cfg(not(feature = "whisper-cpp"))]
            {
                Err(SttError::Config(
                    "provider=whisper_cpp requires building with --features whisper-cpp".into(),
                ))
            }
        }
    }
}

#[cfg(feature = "whisper-cpp")]
fn num_cpus_hint() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
