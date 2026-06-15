use std::sync::Arc;

use lark_kit::{LarkBotClient, LarkConfig, TomlLark};
use larkstack_core::{LarkRegistry, StateStore};
use reqwest::Client;
use serde::Deserialize;

/// GitLab webhook source configuration — secrets/bindings only. Notification routing
/// (rules, destinations, user map, alert labels) is console-editable and lives in the
/// per-App [`StateStore`] via [`lark_kit::routing`].
///
/// At least one of [`webhook_token`](Self::webhook_token) /
/// [`signing_secret`](Self::signing_secret) must be set for the app to start.
#[derive(Debug, Clone, Default)]
pub struct GitLabConfig {
    /// `X-Gitlab-Token` plaintext shared secret (the legacy webhook secret).
    pub webhook_token: String,
    /// GitLab 19.1+ Standard Webhooks signing token (`whsec_…`), verified via the
    /// `webhook-signature` header. `None` = legacy `X-Gitlab-Token` only.
    pub signing_secret: Option<String>,
}

impl GitLabConfig {
    /// Loads from `GITLAB_*` env vars. Used as the base that the `[gitlab]` TOML overlays.
    pub fn from_env() -> Self {
        Self {
            webhook_token: std::env::var("GITLAB_WEBHOOK_TOKEN").unwrap_or_default(),
            signing_secret: std::env::var("GITLAB_SIGNING_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }
}

/// Shared application state, published into the ingress router's [`lark_kit::StateSlot`]
/// while the app runs. Holds the bot (for delivery) and the [`StateStore`] (so the handler
/// loads the live routing config per webhook).
pub struct AppState {
    pub gitlab: GitLabConfig,
    pub bot: Option<LarkBotClient>,
    pub store: Arc<dyn StateStore>,
}

#[derive(Debug, Deserialize, Default)]
struct TopLevel {
    #[serde(rename = "lark-apps", default)]
    lark_apps: LarkRegistry,
    #[serde(default)]
    gitlab: Section,
}

#[derive(Debug, Deserialize, Default)]
struct Section {
    /// Reference into `[lark-apps]`; resolved before the inline `lark` overlay.
    lark_app: Option<String>,
    webhook_token: Option<String>,
    signing_secret: Option<String>,
    #[serde(default)]
    lark: TomlLark,
}

impl AppState {
    /// Build state from a full config TOML containing a `[gitlab]` section, plus the per-App
    /// [`StateStore`] (which backs the live routing config).
    pub fn from_toml(
        full_toml: &str,
        store: Arc<dyn StateStore>,
    ) -> Result<Self, Box<figment::Error>> {
        let top: TopLevel =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;
        let section = top.gitlab;

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

        let mut gitlab = GitLabConfig::from_env();
        if let Some(v) = section.webhook_token {
            gitlab.webhook_token = v;
        }
        if let Some(v) = section.signing_secret {
            gitlab.signing_secret = Some(v).filter(|s| !s.is_empty());
        }

        let http = Client::new();
        let bot = lark.bot_client(&http);
        Ok(Self { gitlab, bot, store })
    }
}
