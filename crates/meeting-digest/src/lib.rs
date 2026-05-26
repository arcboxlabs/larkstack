//! Meeting digest bot for Lark/Feishu.
//!
//! Listens for `vc.meeting.recording_ready_v1` on the Lark WS long connection,
//! downloads the recording, runs Speech-to-Text (pluggable), and posts a
//! digest card pointing at a new Lark Doc containing the full transcript.

pub mod actions;
pub mod audio;
pub mod config;
pub mod events;
pub mod lark;
pub mod pipeline;
pub mod run;
pub mod stt;

pub use actions::handle_actions;
pub use config::{AppConfig, DigestConfig, LarkConfig, SttConfig, SttProvider};
pub use run::{build_pipeline, run, run_ws};
