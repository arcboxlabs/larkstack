use std::collections::HashMap;

use lark_kit::{LarkBotClient, LarkConfig, TomlLark};
use larkstack_core::LarkRegistry;
use reqwest::Client;
use serde::Deserialize;
use tracing::info;

fn default_alert_labels() -> Vec<String> {
    vec!["bug".into(), "urgent".into(), "p0".into()]
}

/// GitLab webhook source configuration.
///
/// At least one of [`webhook_token`](Self::webhook_token) /
/// [`signing_secret`](Self::signing_secret) must be set for the app to start.
#[derive(Debug, Clone)]
pub struct GitLabConfig {
    /// `X-Gitlab-Token` plaintext shared secret (the legacy webhook secret).
    pub webhook_token: String,
    /// GitLab 19.1+ Standard Webhooks signing token (`whsec_…`), verified via the
    /// `webhook-signature` header. `None` = legacy `X-Gitlab-Token` only.
    pub signing_secret: Option<String>,
    /// GitLab username → Lark email, for merge-request review DMs and `<at>` mentions.
    pub user_map: HashMap<String, String>,
    /// Issue label titles (lowercased) that trigger an alert card when applied.
    pub alert_labels: Vec<String>,
    /// `project.path_with_namespace` values to accept events from. Empty = all projects.
    pub project_whitelist: Vec<String>,
}

impl Default for GitLabConfig {
    fn default() -> Self {
        Self {
            webhook_token: String::new(),
            signing_secret: None,
            user_map: HashMap::new(),
            alert_labels: default_alert_labels(),
            project_whitelist: Vec::new(),
        }
    }
}

impl GitLabConfig {
    /// Loads from `GITLAB_*` env vars (secrets empty when unset). Used as the
    /// base that the `[gitlab]` TOML section overlays.
    pub fn from_env() -> Self {
        Self {
            webhook_token: std::env::var("GITLAB_WEBHOOK_TOKEN").unwrap_or_default(),
            signing_secret: std::env::var("GITLAB_SIGNING_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            user_map: std::env::var("GITLAB_USER_MAP")
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            alert_labels: std::env::var("GITLAB_ALERT_LABELS")
                .ok()
                .map(|s| split_csv_lower(&s))
                .unwrap_or_else(default_alert_labels),
            project_whitelist: std::env::var("GITLAB_PROJECT_WHITELIST")
                .ok()
                .map(|s| split_csv(&s))
                .unwrap_or_default(),
        }
    }
}

/// Shared application state, wrapped in `Arc` and published into the ingress
/// router's [`lark_kit::StateSlot`] while the app runs.
pub struct AppState {
    pub gitlab: GitLabConfig,
    pub lark: LarkConfig,
    pub http: Client,
    pub bot: Option<LarkBotClient>,
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
    user_map: Option<HashMap<String, String>>,
    alert_labels: Option<Vec<String>>,
    project_whitelist: Option<Vec<String>>,
    #[serde(default)]
    lark: TomlLark,
}

impl AppState {
    /// Build state from a full config TOML containing a `[gitlab]` section.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
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
        if let Some(v) = section.user_map {
            gitlab.user_map = v;
        }
        if let Some(v) = section.alert_labels {
            gitlab.alert_labels = v.iter().map(|l| l.trim().to_lowercase()).collect();
        }
        if let Some(v) = section.project_whitelist {
            gitlab.project_whitelist = v;
        }

        Ok(Self::from_parts(gitlab, lark))
    }

    fn from_parts(gitlab: GitLabConfig, lark: LarkConfig) -> Self {
        let http = Client::new();
        let bot = lark.bot_client(&http);
        if !gitlab.project_whitelist.is_empty() {
            info!("GitLab project whitelist: {:?}", gitlab.project_whitelist);
        }
        Self {
            gitlab,
            lark,
            http,
            bot,
        }
    }
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect()
}

fn split_csv_lower(s: &str) -> Vec<String> {
    s.split(',')
        .map(|r| r.trim().to_lowercase())
        .filter(|r| !r.is_empty())
        .collect()
}
