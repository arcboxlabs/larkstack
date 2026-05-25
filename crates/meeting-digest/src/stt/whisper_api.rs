//! OpenAI-compatible Whisper HTTP API implementation.
//!
//! Calls `POST {base}/audio/transcriptions` with `response_format=verbose_json`
//! to get segment-level timestamps. Compatible with any provider that mimics
//! this endpoint (OpenAI, Groq, etc.).

use std::path::Path;

use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;

use super::{Segment, SpeechToText, SttError, TranscribeOptions, Transcript};

const UPLOAD_LIMIT_BYTES: u64 = 25 * 1024 * 1024;

pub struct WhisperApi {
    base: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl WhisperApi {
    pub fn new(base: String, api_key: String, model: String) -> Self {
        Self {
            base,
            api_key,
            model,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    segments: Vec<ApiSegment>,
}

#[derive(Debug, Deserialize)]
struct ApiSegment {
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    message: String,
}

#[async_trait]
impl SpeechToText for WhisperApi {
    fn name(&self) -> &'static str {
        "whisper-api"
    }

    async fn transcribe(
        &self,
        input: &Path,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, SttError> {
        let meta = tokio::fs::metadata(input).await?;
        if meta.len() > UPLOAD_LIMIT_BYTES {
            return Err(SttError::Config(format!(
                "input {} is {} bytes (>25MB Whisper API limit); re-encode at a lower bitrate or chunk",
                input.display(),
                meta.len()
            )));
        }

        let bytes = tokio::fs::read(input).await?;
        let filename = input
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio.mp3".to_string());

        let mime = mime_from_ext(input).unwrap_or("application/octet-stream");
        let file_part = Part::bytes(bytes)
            .file_name(filename)
            .mime_str(mime)
            .map_err(|e| SttError::Config(format!("mime: {e}")))?;

        let mut form = Form::new()
            .text("model", self.model.clone())
            .text("response_format", "verbose_json")
            .text("timestamp_granularities[]", "segment")
            .part("file", file_part);

        if let Some(lang) = &opts.language {
            form = form.text("language", lang.clone());
        }
        if let Some(prompt) = &opts.prompt {
            form = form.text("prompt", prompt.clone());
        }

        let url = format!("{}/audio/transcriptions", self.base.trim_end_matches('/'));
        let res = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = res.status();
        let body = res.bytes().await?;
        if !status.is_success() {
            let msg = serde_json::from_slice::<ApiError>(&body)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&body).to_string());
            return Err(SttError::Api {
                code: status.as_u16(),
                message: msg,
            });
        }

        let parsed: ApiResponse = serde_json::from_slice(&body)?;
        let segments = parsed
            .segments
            .into_iter()
            .map(|s| Segment {
                start_ms: (s.start * 1000.0) as u64,
                end_ms: (s.end * 1000.0) as u64,
                text: s.text.trim().to_string(),
            })
            .collect();

        Ok(Transcript {
            language: parsed.language,
            full_text: parsed.text,
            segments,
        })
    }
}

fn mime_from_ext(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => Some("audio/mpeg"),
        Some("wav") => Some("audio/wav"),
        Some("m4a") => Some("audio/mp4"),
        Some("mp4") => Some("video/mp4"),
        Some("ogg" | "oga") => Some("audio/ogg"),
        Some("webm") => Some("audio/webm"),
        Some("flac") => Some("audio/flac"),
        _ => None,
    }
}
