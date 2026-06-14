//! End-to-end pipeline: meeting_id → recording URL → download → audio extract
//! → STT → Lark Doc + digest card.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use larkoapi::{LarkBotClient, MeetingMeta, RecordingFile};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::audio::{self, Target};
use crate::config::{DigestConfig, SttConfig, SttProvider};
use crate::lark::card::{DigestCardInput, build_digest_card};
use crate::lark::docs::{TranscriptDocInput, create_transcript_doc};
use crate::stt::{SpeechToText, TranscribeOptions};

pub struct Pipeline {
    pub client: Arc<LarkBotClient>,
    pub stt: Arc<dyn SpeechToText>,
    pub stt_cfg: SttConfig,
    pub digest_cfg: DigestConfig,
    pub http: reqwest::Client,
}

pub struct Outcome {
    pub meeting_id: String,
    pub doc_url: String,
    pub doc_id: String,
    pub segments: usize,
}

impl Pipeline {
    pub async fn process_meeting(
        &self,
        meeting_id: &str,
        owner_open_id_hint: Option<&str>,
        recording_url_hint: Option<&str>,
    ) -> Result<Outcome, String> {
        info!(meeting_id, "digest: start");

        if self.digest_cfg.folder_token.is_empty() {
            return Err("DIGEST_FOLDER_TOKEN is required".into());
        }

        // 1. resolve meeting meta + recording URL.
        let meta = self
            .client
            .get_meeting(meeting_id)
            .await
            .map_err(|e| format!("get_meeting: {e}"))?;

        let recording = match recording_url_hint {
            Some(url) => RecordingFile {
                url: url.to_string(),
                duration_ms: meta.end_time_ms.saturating_sub(meta.start_time_ms),
            },
            None => self
                .client
                .get_recording(meeting_id)
                .await
                .map_err(|e| format!("get_recording: {e}"))?,
        };
        info!(
            meeting_id,
            duration_ms = recording.duration_ms,
            "digest: recording resolved"
        );

        // 2. download to a temp file.
        let work = self.work_dir();
        tokio::fs::create_dir_all(&work)
            .await
            .map_err(|e| e.to_string())?;

        let media_path = work.join(format!("{meeting_id}.src"));
        download_to(&self.http, &recording.url, &media_path)
            .await
            .map_err(|e| format!("download: {e}"))?;
        info!(meeting_id, path = %media_path.display(), "digest: downloaded");

        // 3. extract audio in the shape the STT backend wants.
        let (audio_path, target) = match self.stt_cfg.provider {
            SttProvider::WhisperApi => (
                work.join(format!("{meeting_id}.mp3")),
                Target::Mp3_16kMono64k,
            ),
            SttProvider::WhisperCpp => (work.join(format!("{meeting_id}.wav")), Target::Wav16kMono),
        };
        audio::extract(&self.digest_cfg.ffmpeg, &media_path, &audio_path, target)
            .await
            .map_err(|e| format!("ffmpeg: {e}"))?;
        info!(meeting_id, path = %audio_path.display(), "digest: audio ready");

        // 4. transcribe.
        let opts = TranscribeOptions::from_config(&self.stt_cfg);
        let transcript = self
            .stt
            .transcribe(&audio_path, &opts)
            .await
            .map_err(|e| format!("stt: {e}"))?;
        info!(
            meeting_id,
            backend = self.stt.name(),
            segments = transcript.segments.len(),
            "digest: transcript ready"
        );

        // 5. create transcript doc in configured folder.
        let doc_title = doc_title(&meta, meeting_id);
        let doc = create_transcript_doc(
            &self.client,
            TranscriptDocInput {
                folder_token: &self.digest_cfg.folder_token,
                title: doc_title,
                topic: meta_topic(&meta),
                duration_ms: recording.duration_ms,
                recording_url: Some(&recording.url),
                transcript: &transcript,
            },
        )
        .await
        .map_err(|e| format!("create_transcript_doc: {e}"))?;
        info!(meeting_id, doc_id = %doc.doc_id, url = %doc.url, "digest: doc created");

        // 6. send digest card.
        let card = build_digest_card(DigestCardInput {
            topic: meta_topic(&meta),
            duration_ms: recording.duration_ms,
            doc_url: &doc.url,
            recording_url: Some(&recording.url),
            transcript: &transcript,
        });

        let recipient = owner_open_id_hint
            .map(String::from)
            .or(meta.owner_open_id.clone());
        match recipient {
            Some(open_id) => {
                if let Err(e) = self
                    .client
                    .send_interactive_returning_id(&open_id, "open_id", &card)
                    .await
                {
                    warn!(meeting_id, %open_id, "digest: DM owner failed: {e}");
                }
            }
            None => warn!(meeting_id, "digest: no owner open_id, skipping DM"),
        }

        if let Some(chat) = self.digest_cfg.fallback_chat_id.as_deref()
            && let Err(e) = self
                .client
                .send_interactive_returning_id(chat, "chat_id", &card)
                .await
        {
            warn!(meeting_id, %chat, "digest: fallback chat post failed: {e}");
        }

        // 7. clean up temp files — ignore errors.
        let _ = tokio::fs::remove_file(&media_path).await;
        let _ = tokio::fs::remove_file(&audio_path).await;

        Ok(Outcome {
            meeting_id: meeting_id.to_string(),
            doc_url: doc.url,
            doc_id: doc.doc_id,
            segments: transcript.segments.len(),
        })
    }

    fn work_dir(&self) -> PathBuf {
        match &self.digest_cfg.work_dir {
            Some(d) => PathBuf::from(d),
            None => std::env::temp_dir().join("minutes"),
        }
    }
}

fn meta_topic(m: &MeetingMeta) -> &str {
    if m.topic.is_empty() {
        "Untitled meeting"
    } else {
        &m.topic
    }
}

fn doc_title(m: &MeetingMeta, meeting_id: &str) -> String {
    let topic = meta_topic(m);
    if m.start_time_ms > 0 {
        let dt = DateTime::<Utc>::from_timestamp_millis(m.start_time_ms as i64);
        if let Some(dt) = dt {
            return format!("{} · {}", dt.format("%Y-%m-%d"), topic);
        }
    }
    format!("{topic} · {meeting_id}")
}

async fn download_to(http: &reqwest::Client, url: &str, dest: &Path) -> Result<(), String> {
    use futures_util::StreamExt;

    let res = http
        .get(url)
        .send()
        .await
        .map_err(|e| format!("http send: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("http {} for {url}", res.status()));
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("create {}: {e}", dest.display()))?;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("stream: {e}"))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| format!("write: {e}"))?;
    }
    file.flush().await.map_err(|e| format!("flush: {e}"))?;
    Ok(())
}
