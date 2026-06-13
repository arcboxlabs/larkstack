use std::collections::HashMap;

use figment::{Figment, providers::Env};
use larkstack_core::LarkRegistry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::debounce::DebounceMap;
use crate::{
    sinks::lark::LarkBotClient,
    sources::{linear::client::LinearClient, x::XClient},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct LinearConfig {
    pub webhook_secret: String,
    pub api_key: Option<String>,
}

impl LinearConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(Env::prefixed("LINEAR_"))
            .extract()
            .map_err(Box::new)
    }

    pub fn graphql_client(&self, http: &Client) -> Option<LinearClient> {
        self.api_key.as_ref().map(|key| {
            info!("LINEAR_API_KEY set – link preview enabled");
            LinearClient::new(key.clone(), http.clone())
        })
    }
}

fn default_lark_base_url() -> String {
    "https://open.larksuite.com".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LarkConfig {
    /// Incoming webhook URL for the Linear notification group chat.
    #[serde(default)]
    pub webhook_url: String,
    /// Incoming webhook URL for GitHub notifications. Falls back to
    /// `webhook_url` when empty.
    #[serde(default)]
    pub github_webhook_url: String,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    /// Verification token for the Linear link-preview app's event callbacks.
    pub verification_token: Option<String>,
    /// Verification token for the X link-preview app (a separate Lark app);
    /// callbacks carrying either token are accepted.
    pub x_verification_token: Option<String>,
    /// Encrypt key for the X link-preview app — decrypts AES-256-CBC callbacks.
    pub x_encrypt_key: Option<String>,
    #[serde(default = "default_lark_base_url")]
    pub base_url: String,
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            webhook_url: String::new(),
            github_webhook_url: String::new(),
            app_id: None,
            app_secret: None,
            verification_token: None,
            x_verification_token: None,
            x_encrypt_key: None,
            base_url: default_lark_base_url(),
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
            _ => {
                info!("LARK_APP_ID/LARK_APP_SECRET not set – DM notifications disabled");
                None
            }
        }
    }
}

fn default_port() -> u16 {
    3000
}

fn default_debounce() -> u64 {
    5000
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_debounce")]
    pub debounce_delay_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            debounce_delay_ms: default_debounce(),
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(figment::providers::Serialized::defaults(Self::default()))
            .merge(Env::raw().only(&["PORT", "DEBOUNCE_DELAY_MS"]))
            .extract()
            .map_err(Box::new)
    }
}

fn default_alert_labels() -> Vec<String> {
    vec!["bug".into(), "urgent".into(), "p0".into()]
}

/// GitHub webhook source configuration. Present only when a webhook secret is
/// set — its absence disables the `/github/webhook` endpoint.
#[derive(Debug)]
pub struct GitHubConfig {
    /// HMAC secret for `X-Hub-Signature-256` verification.
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
    /// Loads from `GITHUB_*` env vars. `None` when `GITHUB_WEBHOOK_SECRET` is
    /// unset/empty (the source stays disabled).
    pub fn from_env() -> Option<Self> {
        let webhook_secret = std::env::var("GITHUB_WEBHOOK_SECRET")
            .ok()
            .filter(|s| !s.is_empty())?;

        let user_map = std::env::var("GITHUB_USER_MAP")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let alert_labels = std::env::var("GITHUB_ALERT_LABELS")
            .ok()
            .map(|s| split_csv_lower(&s))
            .unwrap_or_else(default_alert_labels);

        let repo_whitelist = std::env::var("GITHUB_REPO_WHITELIST")
            .ok()
            .map(|s| split_csv(&s))
            .unwrap_or_default();

        Some(Self {
            webhook_secret,
            user_map,
            alert_labels,
            repo_whitelist,
        })
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

/// Shared application state, wrapped in `Arc` and passed to every handler.
pub struct AppState {
    pub linear: LinearConfig,
    pub lark: LarkConfig,
    pub server: ServerConfig,
    pub github: Option<GitHubConfig>,
    pub http: Client,
    pub lark_bot: Option<LarkBotClient>,
    pub linear_client: Option<LinearClient>,
    pub x_client: XClient,
    pub update_debounce: DebounceMap,
}

#[derive(Debug, Deserialize, Default)]
struct TopLevelToml {
    #[serde(rename = "lark-apps", default)]
    lark_apps: LarkRegistry,
    #[serde(rename = "linear-bridge", default)]
    linear_bridge: TomlSection,
}

#[derive(Debug, Deserialize, Default)]
struct TomlSection {
    #[serde(default)]
    linear: TomlLinear,
    /// Reference into `[lark-apps]`; resolved before the inline overlay.
    lark_app: Option<String>,
    #[serde(default)]
    lark: TomlLark,
    #[serde(default)]
    server: TomlServer,
    #[serde(default)]
    github: TomlGitHub,
}

#[derive(Debug, Deserialize, Default)]
struct TomlLinear {
    webhook_secret: Option<String>,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlLark {
    webhook_url: Option<String>,
    github_webhook_url: Option<String>,
    app_id: Option<String>,
    app_secret: Option<String>,
    verification_token: Option<String>,
    x_verification_token: Option<String>,
    x_encrypt_key: Option<String>,
    base_url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlServer {
    port: Option<u16>,
    debounce_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct TomlGitHub {
    webhook_secret: Option<String>,
    user_map: Option<HashMap<String, String>>,
    alert_labels: Option<Vec<String>>,
    repo_whitelist: Option<Vec<String>>,
}

impl AppState {
    pub fn from_env() -> Self {
        Self::from_parts(
            LinearConfig::from_env().expect("invalid linear config"),
            LarkConfig::from_env().expect("invalid lark config"),
            ServerConfig::from_env().expect("invalid server config"),
            GitHubConfig::from_env(),
        )
    }

    /// Build state from a full config TOML containing a `[linear-bridge]`
    /// section. Any field missing from the TOML falls back to the env-var
    /// loader, so callers can pass partial configs without losing secrets
    /// that only live in the env.
    pub fn from_toml(full_toml: &str) -> Result<Self, Box<figment::Error>> {
        let top: TopLevelToml =
            toml::from_str(full_toml).map_err(|e| Box::new(figment::Error::from(e.to_string())))?;
        let parsed = top.linear_bridge;

        let mut linear = LinearConfig::from_env().unwrap_or(LinearConfig {
            webhook_secret: String::new(),
            api_key: None,
        });
        if let Some(s) = parsed.linear.webhook_secret {
            linear.webhook_secret = s;
        }
        if parsed.linear.api_key.is_some() {
            linear.api_key = parsed.linear.api_key;
        }

        let mut lark = LarkConfig::from_env().unwrap_or_default();
        if let Some(name) = &parsed.lark_app {
            let app = top.lark_apps.get(name).ok_or_else(|| {
                Box::new(figment::Error::from(format!(
                    "lark_app '{name}' not found in [lark-apps]"
                )))
            })?;
            lark.app_id = Some(app.app_id.clone());
            lark.app_secret = Some(app.app_secret.clone());
            lark.base_url = app.base_url.clone();
        }
        if let Some(s) = parsed.lark.webhook_url {
            lark.webhook_url = s;
        }
        if let Some(s) = parsed.lark.github_webhook_url {
            lark.github_webhook_url = s;
        }
        if parsed.lark.app_id.is_some() {
            lark.app_id = parsed.lark.app_id;
        }
        if parsed.lark.app_secret.is_some() {
            lark.app_secret = parsed.lark.app_secret;
        }
        if parsed.lark.verification_token.is_some() {
            lark.verification_token = parsed.lark.verification_token;
        }
        if parsed.lark.x_verification_token.is_some() {
            lark.x_verification_token = parsed.lark.x_verification_token;
        }
        if parsed.lark.x_encrypt_key.is_some() {
            lark.x_encrypt_key = parsed.lark.x_encrypt_key;
        }
        if let Some(s) = parsed.lark.base_url {
            lark.base_url = s;
        }

        let mut server = ServerConfig::from_env().unwrap_or_default();
        if let Some(p) = parsed.server.port {
            server.port = p;
        }
        if let Some(d) = parsed.server.debounce_delay_ms {
            server.debounce_delay_ms = d;
        }

        // GitHub: env baseline, overlaid by [linear-bridge.github]. Disabled
        // (None) unless a webhook secret ends up set.
        let tg = parsed.github;
        let mut github = GitHubConfig::from_env();
        if tg.webhook_secret.is_some()
            || tg.user_map.is_some()
            || tg.alert_labels.is_some()
            || tg.repo_whitelist.is_some()
        {
            let mut g = github.take().unwrap_or_default();
            if let Some(s) = tg.webhook_secret {
                g.webhook_secret = s;
            }
            if let Some(m) = tg.user_map {
                g.user_map = m;
            }
            if let Some(l) = tg.alert_labels {
                g.alert_labels = l.iter().map(|s| s.trim().to_lowercase()).collect();
            }
            if let Some(w) = tg.repo_whitelist {
                g.repo_whitelist = w;
            }
            github = Some(g);
        }
        let github = github.filter(|g| !g.webhook_secret.is_empty());

        Ok(Self::from_parts(linear, lark, server, github))
    }

    fn from_parts(
        linear: LinearConfig,
        lark: LarkConfig,
        server: ServerConfig,
        github: Option<GitHubConfig>,
    ) -> Self {
        let http = Client::new();
        let lark_bot = lark.bot_client(&http);
        let linear_client = linear.graphql_client(&http);
        let x_client = XClient::new(std::env::var("X_BEARER_TOKEN").ok(), http.clone());

        if lark.verification_token.is_some() {
            info!("LARK_VERIFICATION_TOKEN set – event verification enabled");
        }
        if lark.x_encrypt_key.is_some() {
            info!("LARK_X_ENCRYPT_KEY set – encrypted X event callbacks enabled");
        }
        if let Some(gh) = &github {
            info!("GITHUB_WEBHOOK_SECRET set – GitHub webhook source enabled");
            if !gh.repo_whitelist.is_empty() {
                info!("GitHub repo whitelist: {:?}", gh.repo_whitelist);
            }
        }
        info!("debounce delay: {}ms", server.debounce_delay_ms);

        Self {
            linear,
            lark,
            server,
            github,
            http,
            lark_bot,
            linear_client,
            x_client,
            update_debounce: DebounceMap::new(),
        }
    }
}
