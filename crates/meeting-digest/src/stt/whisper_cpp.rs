//! Local whisper.cpp implementation via the `whisper-rs` bindings.
//!
//! Expects 16kHz mono PCM. `pipeline::audio` produces a WAV of that shape, so
//! we just decode it with `hound` and feed f32 samples into the model.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{Segment, SpeechToText, SttError, TranscribeOptions, Transcript};

pub struct WhisperCpp {
    ctx: Arc<WhisperContext>,
    threads: u32,
}

impl WhisperCpp {
    pub fn new(model_path: String, threads: u32) -> Result<Self, SttError> {
        let path = PathBuf::from(&model_path);
        if !path.is_file() {
            return Err(SttError::Config(format!(
                "whisper model not found: {model_path}"
            )));
        }
        let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            .map_err(|e| SttError::WhisperCpp(format!("load model: {e}")))?;
        Ok(Self {
            ctx: Arc::new(ctx),
            threads,
        })
    }
}

#[async_trait]
impl SpeechToText for WhisperCpp {
    fn name(&self) -> &'static str {
        "whisper-cpp"
    }

    async fn transcribe(
        &self,
        input: &Path,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, SttError> {
        let input = input.to_path_buf();
        let ctx = Arc::clone(&self.ctx);
        let threads = self.threads;
        let language = opts.language.clone();
        let prompt = opts.prompt.clone();

        task::spawn_blocking(move || run_blocking(ctx, threads, input, language, prompt))
            .await
            .map_err(|e| SttError::WhisperCpp(format!("join: {e}")))?
    }
}

fn run_blocking(
    ctx: Arc<WhisperContext>,
    threads: u32,
    input: PathBuf,
    language: Option<String>,
    prompt: Option<String>,
) -> Result<Transcript, SttError> {
    let samples = read_wav_f32_mono_16k(&input)?;

    let mut state = ctx
        .create_state()
        .map_err(|e| SttError::WhisperCpp(format!("state: {e}")))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads as i32);
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    if let Some(lang) = language.as_deref() {
        params.set_language(Some(lang));
    }
    if let Some(p) = prompt.as_deref() {
        params.set_initial_prompt(p);
    }

    state
        .full(params, &samples)
        .map_err(|e| SttError::WhisperCpp(format!("decode: {e}")))?;

    let n = state
        .full_n_segments()
        .map_err(|e| SttError::WhisperCpp(format!("n_segments: {e}")))?;
    let mut segments = Vec::with_capacity(n as usize);
    let mut full = String::new();
    for i in 0..n {
        let text = state
            .full_get_segment_text(i)
            .map_err(|e| SttError::WhisperCpp(format!("segment text: {e}")))?;
        let start = state
            .full_get_segment_t0(i)
            .map_err(|e| SttError::WhisperCpp(format!("segment t0: {e}")))?
            as u64
            * 10; // whisper.cpp timestamps are in 10ms units
        let end = state
            .full_get_segment_t1(i)
            .map_err(|e| SttError::WhisperCpp(format!("segment t1: {e}")))?
            as u64
            * 10;
        full.push_str(&text);
        segments.push(Segment {
            start_ms: start,
            end_ms: end,
            text: text.trim().to_string(),
        });
    }

    let detected_lang = state
        .full_lang_id()
        .ok()
        .and_then(|id| whisper_rs::get_lang_str(id))
        .map(|s| s.to_string());

    Ok(Transcript {
        language: detected_lang.or(language),
        full_text: full,
        segments,
    })
}

fn read_wav_f32_mono_16k(path: &Path) -> Result<Vec<f32>, SttError> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| SttError::WhisperCpp(format!("wav open {}: {e}", path.display())))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 {
        return Err(SttError::WhisperCpp(format!(
            "expected 16kHz, got {}",
            spec.sample_rate
        )));
    }
    if spec.channels != 1 {
        return Err(SttError::WhisperCpp(format!(
            "expected mono, got {} channels",
            spec.channels
        )));
    }

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SttError::WhisperCpp(format!("read f32: {e}")))?,
        hound::SampleFormat::Int => {
            let max = 2_f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| SttError::WhisperCpp(format!("read i32: {e}")))?
        }
    };
    Ok(samples)
}
