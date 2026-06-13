use lark_kit::{LarkConfig, ServerConfig, TomlLark};
use larkstack_core::LarkRegistry;
use serde::Deserialize;

use crate::source::XClient;

/// Shared application state, wrapped in `Arc` and passed to every handler.
pub struct AppState {
    pub lark: LarkConfig,
    pub server: ServerConfig,
    pub x: XClient,
}

#[derive(Debug, Deserialize, Default)]
struct TopLevel {
    #[serde(rename = "lark-apps", default)]
    lark_apps: LarkRegistry,
    #[serde(default)]
    x: Section,
}

#[derive(Debug, Deserialize, Default)]
struct Section {
    /// Reference into `[lark-apps]`; resolved before the inline `lark` overlay.
    lark_app: Option<String>,
    #[serde(default)]
    lark: TomlLark,
    #[serde(default)]
    server: TomlServer,
}

#[derive(Debug, Deserialize, Default)]
struct TomlServer {
    port: Option<u16>,
}

impl AppState {
    pub fn from_env() -> Self {
        Self::from_parts(
            LarkConfig::from_env().expect("invalid lark config"),
            ServerConfig::from_env().expect("invalid server config"),
        )
    }

    /// Build state from a full config TOML containing an `[x]` section.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
        let top: TopLevel =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;
        let section = top.x;

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

        Ok(Self::from_parts(lark, server))
    }

    fn from_parts(lark: LarkConfig, server: ServerConfig) -> Self {
        let http = reqwest::Client::new();
        let x = XClient::new(std::env::var("X_BEARER_TOKEN").ok(), http);
        Self { lark, server, x }
    }
}
