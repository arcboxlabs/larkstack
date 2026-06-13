use lark_kit::{LarkBotClient, LarkConfig, ServerConfig, TomlLark};
use larkstack_core::LarkRegistry;
use reqwest::Client;
use serde::Deserialize;
use tracing::info;

use crate::debounce::DebounceMap;
use crate::source::client::LinearClient;

fn default_debounce() -> u64 {
    5000
}

/// Linear webhook source configuration.
#[derive(Debug)]
pub struct LinearConfig {
    /// HMAC secret for `linear-signature` verification.
    pub webhook_secret: String,
    /// GraphQL API key — enables issue link previews when set.
    pub api_key: Option<String>,
}

impl LinearConfig {
    pub fn from_env() -> Self {
        Self {
            webhook_secret: std::env::var("LINEAR_WEBHOOK_SECRET").unwrap_or_default(),
            api_key: std::env::var("LINEAR_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }

    pub fn graphql_client(&self, http: &Client) -> Option<LinearClient> {
        self.api_key.as_ref().map(|key| {
            info!("LINEAR_API_KEY set – link preview enabled");
            LinearClient::new(key.clone(), http.clone())
        })
    }
}

/// Shared application state, wrapped in `Arc` and passed to every handler.
pub struct AppState {
    pub linear: LinearConfig,
    pub lark: LarkConfig,
    pub server: ServerConfig,
    pub debounce_delay_ms: u64,
    pub http: Client,
    pub bot: Option<LarkBotClient>,
    pub linear_client: Option<LinearClient>,
    pub debounce: DebounceMap,
}

#[derive(Debug, Deserialize, Default)]
struct TopLevel {
    #[serde(rename = "lark-apps", default)]
    lark_apps: LarkRegistry,
    #[serde(default)]
    linear: Section,
}

#[derive(Debug, Deserialize, Default)]
struct Section {
    /// Reference into `[lark-apps]`; resolved before the inline `lark` overlay.
    lark_app: Option<String>,
    webhook_secret: Option<String>,
    api_key: Option<String>,
    #[serde(default)]
    lark: TomlLark,
    #[serde(default)]
    server: TomlServer,
}

#[derive(Debug, Deserialize, Default)]
struct TomlServer {
    port: Option<u16>,
    debounce_delay_ms: Option<u64>,
}

impl AppState {
    pub fn from_env() -> Self {
        let debounce_delay_ms = std::env::var("DEBOUNCE_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(default_debounce);
        Self::from_parts(
            LinearConfig::from_env(),
            LarkConfig::from_env().expect("invalid lark config"),
            ServerConfig::from_env().expect("invalid server config"),
            debounce_delay_ms,
        )
    }

    /// Build state from a full config TOML containing a `[linear]` section.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
        let top: TopLevel =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;
        let section = top.linear;

        let mut linear = LinearConfig::from_env();
        if let Some(v) = section.webhook_secret {
            linear.webhook_secret = v;
        }
        if section.api_key.is_some() {
            linear.api_key = section.api_key;
        }

        let mut lark = LarkConfig::from_env().unwrap_or_default();
        if let Some(name) = &section.lark_app {
            let app = top.lark_apps.get(name).ok_or_else(|| {
                Box::new(figment::Error::from(format!(
                    "lark_app '{name}' not found in [lark-apps]"
                )))
            })?;
            lark.apply_lark_app(app);
        }
        lark.overlay(section.lark);

        let mut server = ServerConfig::from_env().unwrap_or_default();
        if let Some(p) = section.server.port {
            server.port = p;
        }
        let debounce_delay_ms = section.server.debounce_delay_ms.unwrap_or_else(|| {
            std::env::var("DEBOUNCE_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(default_debounce)
        });

        Ok(Self::from_parts(linear, lark, server, debounce_delay_ms))
    }

    fn from_parts(
        linear: LinearConfig,
        lark: LarkConfig,
        server: ServerConfig,
        debounce_delay_ms: u64,
    ) -> Self {
        let http = Client::new();
        let bot = lark.bot_client(&http);
        let linear_client = linear.graphql_client(&http);
        info!("debounce delay: {debounce_delay_ms}ms");
        Self {
            linear,
            lark,
            server,
            debounce_delay_ms,
            http,
            bot,
            linear_client,
            debounce: DebounceMap::new(),
        }
    }
}
