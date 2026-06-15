use std::sync::Arc;

use lark_kit::{LarkBotClient, LarkConfig, TomlLark};
use larkstack_core::{LarkRegistry, StateStore};
use reqwest::Client;
use serde::Deserialize;

/// GitHub webhook source configuration — secrets/bindings only. Notification routing
/// (rules, destinations, user map, alert labels) is console-editable and lives in the
/// per-App [`StateStore`] via [`lark_kit::routing`].
#[derive(Debug, Clone, Default)]
pub struct GitHubConfig {
    /// HMAC secret for `X-Hub-Signature-256` verification. Empty disables the app.
    pub webhook_secret: String,
}

impl GitHubConfig {
    /// Loads from `GITHUB_*` env vars. Used as the base that the `[github]` TOML overlays.
    pub fn from_env() -> Self {
        Self {
            webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET").unwrap_or_default(),
        }
    }
}

/// Shared application state, published into the ingress router's [`lark_kit::StateSlot`]
/// while the app runs. Holds the bot (for delivery) and the [`StateStore`] (so the handler
/// loads the live routing config per webhook).
pub struct AppState {
    pub github: GitHubConfig,
    pub bot: Option<Arc<LarkBotClient>>,
    pub store: Arc<dyn StateStore>,
}

#[derive(Debug, Deserialize, Default)]
struct TopLevel {
    #[serde(rename = "lark-apps", default)]
    lark_apps: LarkRegistry,
    #[serde(default)]
    github: Section,
}

#[derive(Debug, Deserialize, Default)]
struct Section {
    /// Reference into `[lark-apps]`; resolved before the inline `lark` overlay.
    lark_app: Option<String>,
    webhook_secret: Option<String>,
    #[serde(default)]
    lark: TomlLark,
}

impl AppState {
    /// Build state from a full config TOML containing a `[github]` section, plus the per-App
    /// [`StateStore`] (which backs the live routing config).
    pub fn from_toml(
        full_toml: &str,
        store: Arc<dyn StateStore>,
    ) -> Result<Self, Box<figment::Error>> {
        let top: TopLevel =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;
        let section = top.github;

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

        let mut github = GitHubConfig::from_env();
        if let Some(v) = section.webhook_secret {
            github.webhook_secret = v;
        }

        let http = Client::new();
        let bot = lark.bot_client(&http).map(Arc::new);
        Ok(Self { github, bot, store })
    }
}
