//! ffmpeg wrapper — extracts a 16kHz mono audio track from the downloaded
//! meeting recording. Two outputs:
//!   - WAV (pcm_s16le) for whisper.cpp
//!   - MP3 (64k) for Whisper API (stays under the 25MB upload cap for ~50min
//!     meetings; recode / chunk separately if you push past that).

use std::path::Path;
use std::process::Stdio;

use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("ffmpeg spawn ({cmd}): {source}")]
    Spawn {
        cmd: String,
        #[source]
        source: std::io::Error,
    },
    #[error("ffmpeg ({cmd}) exit {status}: {stderr}")]
    Exit {
        cmd: String,
        status: i32,
        stderr: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum Target {
    Wav16kMono,
    Mp3_16kMono64k,
}

pub async fn extract(
    ffmpeg: &str,
    input: &Path,
    output: &Path,
    target: Target,
) -> Result<(), AudioError> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000");
    match target {
        Target::Wav16kMono => {
            cmd.arg("-c:a").arg("pcm_s16le");
        }
        Target::Mp3_16kMono64k => {
            cmd.arg("-c:a").arg("libmp3lame").arg("-b:a").arg("64k");
        }
    }
    cmd.arg(output);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|e| AudioError::Spawn {
        cmd: ffmpeg.into(),
        source: e,
    })?;
    let out = child
        .wait_with_output()
        .await
        .map_err(|e| AudioError::Spawn {
            cmd: ffmpeg.into(),
            source: e,
        })?;
    if !out.status.success() {
        return Err(AudioError::Exit {
            cmd: ffmpeg.into(),
            status: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(())
}
