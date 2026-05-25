use figment::{Figment, providers::Env};
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
}
