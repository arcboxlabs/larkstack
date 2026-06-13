use figment::{Figment, providers::Env};
use larkstack_core::LarkRegistry;
use serde::{Deserialize, Serialize};

fn default_base_url() -> String {
    "https://open.larksuite.com".to_string()
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StandupConfig {
    #[serde(default)]
    pub enabled: bool,
    pub chat_id: Option<String>,
    pub folder_token: Option<String>,
}

impl StandupConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::prefixed("STANDUP_"))
            .extract()
            .map_err(Box::new)
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub lark: LarkConfig,
    pub standup: StandupConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Ok(Self {
            lark: LarkConfig::from_env()?,
            standup: StandupConfig::from_env()?,
        })
    }

    /// Build from a full config TOML containing a `[standup-bot]` section.
    /// The env loader runs first; TOML overlays per field.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
        #[derive(Default, Deserialize)]
        struct TopLevel {
            #[serde(rename = "lark-apps", default)]
            lark_apps: LarkRegistry,
            #[serde(rename = "standup-bot", default)]
            section: Section,
        }
        #[derive(Default, Deserialize)]
        struct Section {
            /// Reference into `[lark-apps]`; resolved before the inline overlay.
            lark_app: Option<String>,
            #[serde(default)]
            lark: TomlLark,
            #[serde(default)]
            standup: TomlStandup,
        }
        #[derive(Default, Deserialize)]
        struct TomlLark {
            app_id: Option<String>,
            app_secret: Option<String>,
            base_url: Option<String>,
        }
        #[derive(Default, Deserialize)]
        struct TomlStandup {
            enabled: Option<bool>,
            chat_id: Option<String>,
            folder_token: Option<String>,
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

        if let Some(v) = top.section.standup.enabled {
            cfg.standup.enabled = v;
        }
        if top.section.standup.chat_id.is_some() {
            cfg.standup.chat_id = top.section.standup.chat_id;
        }
        if top.section.standup.folder_token.is_some() {
            cfg.standup.folder_token = top.section.standup.folder_token;
        }

        Ok(cfg)
    }
}
