//! Shared Lark + server config, reused by every integration's `AppState`.

use figment::{Figment, providers::Env};
use larkstack_core::{LarkApp, default_base_url};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::bot::LarkBotClient;

/// One integration's Lark side: where it posts, who it posts as, and how it
/// authenticates inbound event callbacks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LarkConfig {
    /// Incoming group-webhook URL for notification cards.
    #[serde(default)]
    pub webhook_url: String,
    /// Self-built app id/secret — enables bot DMs.
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    /// Verification token for this app's event callbacks (`/lark/event`).
    pub verification_token: Option<String>,
    /// Encrypt Key — when set, event callbacks are AES-256-CBC decrypted.
    pub encrypt_key: Option<String>,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            webhook_url: String::new(),
            app_id: None,
            app_secret: None,
            verification_token: None,
            encrypt_key: None,
            base_url: default_base_url(),
        }
    }
}

/// TOML overlay for a `[<app>.lark]` section — every field optional.
#[derive(Debug, Default, Deserialize)]
pub struct TomlLark {
    pub webhook_url: Option<String>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub verification_token: Option<String>,
    pub encrypt_key: Option<String>,
    pub base_url: Option<String>,
}

impl LarkConfig {
    /// Loads from `LARK_*` env vars (the standalone-binary path).
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::prefixed("LARK_"))
            .extract()
            .map_err(Box::new)
    }

    /// Overlays credentials from a `[lark-apps.<name>]` registry entry.
    pub fn apply_lark_app(&mut self, app: &LarkApp) {
        self.app_id = Some(app.app_id.clone());
        self.app_secret = Some(app.app_secret.clone());
        self.base_url = app.base_url.clone();
    }

    /// Overlays any fields present in a `[<app>.lark]` TOML section.
    pub fn overlay(&mut self, t: TomlLark) {
        if let Some(v) = t.webhook_url {
            self.webhook_url = v;
        }
        if t.app_id.is_some() {
            self.app_id = t.app_id;
        }
        if t.app_secret.is_some() {
            self.app_secret = t.app_secret;
        }
        if t.verification_token.is_some() {
            self.verification_token = t.verification_token;
        }
        if t.encrypt_key.is_some() {
            self.encrypt_key = t.encrypt_key;
        }
        if let Some(v) = t.base_url {
            self.base_url = v;
        }
    }

    /// Builds a DM bot client when app credentials are present.
    pub fn bot_client(&self, http: &Client) -> Option<LarkBotClient> {
        match (&self.app_id, &self.app_secret) {
            (Some(id), Some(secret)) => {
                info!("lark bot configured – DM notifications enabled");
                Some(LarkBotClient::new(
                    id.clone(),
                    secret.clone(),
                    self.base_url.clone(),
                    http.clone(),
                ))
            }
            _ => None,
        }
    }
}

fn default_port() -> u16 {
    3000
}

/// The inbound HTTP server's listen port.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::raw().only(&["PORT"]))
            .extract()
            .map_err(Box::new)
    }
}
