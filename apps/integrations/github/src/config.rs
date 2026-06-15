use std::collections::HashMap;

use lark_kit::{LarkBotClient, LarkConfig, TomlLark};
use larkstack_core::LarkRegistry;
use reqwest::Client;
use serde::Deserialize;
use tracing::info;

fn default_alert_labels() -> Vec<String> {
    vec!["bug".into(), "urgent".into(), "p0".into()]
}

/// GitHub webhook source configuration.
#[derive(Debug, Clone)]
pub struct GitHubConfig {
    /// HMAC secret for `X-Hub-Signature-256` verification. Empty disables the app.
    pub webhook_secret: String,
    /// GitHub login → Lark email, for review-request DMs and `<at>` mentions.
    pub user_map: HashMap<String, String>,
    /// Issue labels (lowercased) that trigger an alert card when applied.
    pub alert_labels: Vec<String>,
    /// Repo names to accept events from. Empty = all repos.
    pub repo_whitelist: Vec<String>,
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            webhook_secret: String::new(),
            user_map: HashMap::new(),
            alert_labels: default_alert_labels(),
            repo_whitelist: Vec::new(),
        }
    }
}

impl GitHubConfig {
    /// Loads from `GITHUB_*` env vars (webhook secret empty when unset). Used as
    /// the base that the `[github]` TOML section overlays.
    pub fn from_env() -> Self {
        Self {
            webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET").unwrap_or_default(),
            user_map: std::env::var("GITHUB_USER_MAP")
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            alert_labels: std::env::var("GITHUB_ALERT_LABELS")
                .ok()
                .map(|s| split_csv_lower(&s))
                .unwrap_or_else(default_alert_labels),
            repo_whitelist: std::env::var("GITHUB_REPO_WHITELIST")
                .ok()
                .map(|s| split_csv(&s))
                .unwrap_or_default(),
        }
    }
}

/// Shared application state, wrapped in `Arc` and published into the ingress
/// router's [`lark_kit::StateSlot`] while the app runs.
pub struct AppState {
    pub github: GitHubConfig,
    pub lark: LarkConfig,
    pub http: Client,
    pub bot: Option<LarkBotClient>,
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
    user_map: Option<HashMap<String, String>>,
    alert_labels: Option<Vec<String>>,
    repo_whitelist: Option<Vec<String>>,
    #[serde(default)]
    lark: TomlLark,
}

impl AppState {
    /// Build state from a full config TOML containing a `[github]` section.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
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
        if let Some(v) = section.user_map {
            github.user_map = v;
        }
        if let Some(v) = section.alert_labels {
            github.alert_labels = v.iter().map(|l| l.trim().to_lowercase()).collect();
        }
        if let Some(v) = section.repo_whitelist {
            github.repo_whitelist = v;
        }

        Ok(Self::from_parts(github, lark))
    }

    fn from_parts(github: GitHubConfig, lark: LarkConfig) -> Self {
        let http = Client::new();
        let bot = lark.bot_client(&http);
        if !github.repo_whitelist.is_empty() {
            info!("GitHub repo whitelist: {:?}", github.repo_whitelist);
        }
        Self {
            github,
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
