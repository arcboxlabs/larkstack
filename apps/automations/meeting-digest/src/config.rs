use figment::{Figment, providers::Env};
use larkstack_core::LarkRegistry;
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

    /// Build from a full config TOML containing a `[meeting-digest]` section.
    /// Any field omitted from the TOML falls back to its env-var equivalent;
    /// the env loader runs first and the TOML section overlays on top.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
        #[derive(Default, Deserialize)]
        struct TopLevel {
            #[serde(rename = "lark-apps", default)]
            lark_apps: LarkRegistry,
            #[serde(rename = "meeting-digest", default)]
            section: Section,
        }
        #[derive(Default, Deserialize)]
        struct Section {
            /// Reference into `[lark-apps]`; resolved before the inline overlay.
            lark_app: Option<String>,
            #[serde(default)]
            lark: TomlLark,
            #[serde(default)]
            stt: TomlStt,
            #[serde(default)]
            digest: TomlDigest,
        }
        #[derive(Default, Deserialize)]
        struct TomlLark {
            app_id: Option<String>,
            app_secret: Option<String>,
            base_url: Option<String>,
        }
        #[derive(Default, Deserialize)]
        struct TomlStt {
            provider: Option<SttProvider>,
            language: Option<String>,
            whisper_api_base: Option<String>,
            whisper_api_model: Option<String>,
            whisper_api_key: Option<String>,
            whisper_cpp_model: Option<String>,
            whisper_cpp_threads: Option<u32>,
        }
        #[derive(Default, Deserialize)]
        struct TomlDigest {
            folder_token: Option<String>,
            fallback_chat_id: Option<String>,
            work_dir: Option<String>,
            ffmpeg: Option<String>,
        }

        let top: TopLevel =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;

        let mut cfg = Self::from_env()?;

        if let Some(name) = &top.section.lark_app {
            let app = top.lark_apps.get(name).ok_or_else(|| {
                Box::new(figment::Error::from(format!(
                    "lark_app '{name}' not found in [lark-apps]"
                )))
            })?;
            cfg.lark.app_id = app.app_id.clone();
            cfg.lark.app_secret = app.app_secret.clone();
            cfg.lark.base_url = app.base_url.clone();
        }

        if let Some(v) = top.section.lark.app_id {
            cfg.lark.app_id = v;
        }
        if let Some(v) = top.section.lark.app_secret {
            cfg.lark.app_secret = v;
        }
        if let Some(v) = top.section.lark.base_url {
            cfg.lark.base_url = v;
        }

        if let Some(v) = top.section.stt.provider {
            cfg.stt.provider = v;
        }
        if let Some(v) = top.section.stt.language {
            cfg.stt.language = v;
        }
        if let Some(v) = top.section.stt.whisper_api_base {
            cfg.stt.whisper_api_base = v;
        }
        if let Some(v) = top.section.stt.whisper_api_model {
            cfg.stt.whisper_api_model = v;
        }
        if let Some(v) = top.section.stt.whisper_api_key {
            cfg.stt.whisper_api_key = v;
        }
        if let Some(v) = top.section.stt.whisper_cpp_model {
            cfg.stt.whisper_cpp_model = v;
        }
        if let Some(v) = top.section.stt.whisper_cpp_threads {
            cfg.stt.whisper_cpp_threads = v;
        }

        if let Some(v) = top.section.digest.folder_token {
            cfg.digest.folder_token = v;
        }
        if top.section.digest.fallback_chat_id.is_some() {
            cfg.digest.fallback_chat_id = top.section.digest.fallback_chat_id;
        }
        if top.section.digest.work_dir.is_some() {
            cfg.digest.work_dir = top.section.digest.work_dir;
        }
        if let Some(v) = top.section.digest.ffmpeg {
            cfg.digest.ffmpeg = v;
        }

        Ok(cfg)
    }
}
