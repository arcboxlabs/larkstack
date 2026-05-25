use figment::{Figment, providers::Env};
use serde::{Deserialize, Serialize};

fn default_base_url() -> String {
    "https://open.larksuite.com".to_string()
}

fn default_language() -> String {
    "auto".to_string()
}

fn default_whisper_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_whisper_api_model() -> String {
    "whisper-1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkConfig {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            base_url: default_base_url(),
        }
    }
}

impl LarkConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::prefixed("LARK_"))
            .extract()
            .map_err(Box::new)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttProvider {
    #[default]
    WhisperApi,
    WhisperCpp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    #[serde(default)]
    pub provider: SttProvider,
    #[serde(default = "default_language")]
    pub language: String,

    #[serde(default = "default_whisper_api_base")]
    pub whisper_api_base: String,
    #[serde(default = "default_whisper_api_model")]
    pub whisper_api_model: String,
    #[serde(default)]
    pub whisper_api_key: String,

    /// Path to a ggml `.bin` model file (e.g. `ggml-base.bin`). Required when
    /// provider=whisper_cpp, unused otherwise.
    #[serde(default)]
    pub whisper_cpp_model: String,
    #[serde(default)]
    pub whisper_cpp_threads: u32,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            provider: SttProvider::default(),
            language: default_language(),
            whisper_api_base: default_whisper_api_base(),
            whisper_api_model: default_whisper_api_model(),
            whisper_api_key: String::new(),
            whisper_cpp_model: String::new(),
            whisper_cpp_threads: 0,
        }
    }
}

impl SttConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::prefixed("STT_"))
            .extract()
            .map_err(Box::new)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestConfig {
    /// Folder token where transcript docs are created. Required.
    #[serde(default)]
    pub folder_token: String,
    /// If set, also post the digest card to this chat. Otherwise only DM the meeting owner.
    pub fallback_chat_id: Option<String>,
    /// Working directory for temp audio files. Default: OS temp dir.
    pub work_dir: Option<String>,
    /// ffmpeg binary path. Default: `ffmpeg` in PATH.
    #[serde(default = "default_ffmpeg")]
    pub ffmpeg: String,
}

fn default_ffmpeg() -> String {
    "ffmpeg".to_string()
}

impl Default for DigestConfig {
    fn default() -> Self {
        Self {
            folder_token: String::new(),
            fallback_chat_id: None,
            work_dir: None,
            ffmpeg: default_ffmpeg(),
        }
    }
}

impl DigestConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::prefixed("DIGEST_"))
            .extract()
            .map_err(Box::new)
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub lark: LarkConfig,
    pub stt: SttConfig,
    pub digest: DigestConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Ok(Self {
            lark: LarkConfig::from_env()?,
            stt: SttConfig::from_env()?,
            digest: DigestConfig::from_env()?,
        })
    }
}
