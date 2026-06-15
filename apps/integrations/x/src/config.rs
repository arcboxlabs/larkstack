use lark_kit::{LarkConfig, TomlLark};
use larkstack_core::LarkRegistry;
use serde::Deserialize;

use crate::source::XClient;

/// Shared application state, wrapped in `Arc` and published into the ingress
/// router's [`lark_kit::StateSlot`] while the app runs.
pub struct AppState {
    pub lark: LarkConfig,
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
}

impl AppState {
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

        Ok(Self::from_parts(lark))
    }

    fn from_parts(lark: LarkConfig) -> Self {
        let http = reqwest::Client::new();
        let x = XClient::new(std::env::var("X_BEARER_TOKEN").ok(), http);
        Self { lark, x }
    }
}
