//! Shared Lark-app credential registry.
//!
//! The App contract is deliberately generic, but every larkstack App ultimately
//! drives a Lark application, and several Apps often drive the *same* one. So the
//! set of Lark apps is a framework-level concern, not a per-App one: credentials
//! are registered once under `[lark-apps.<name>]` in the console config (or
//! onboarded from the UI, which live-tests them), and an App binds to one by name.
//!
//! ```toml
//! [lark-apps.main]
//! app_id = "cli_..."
//! app_secret = "..."
//! base_url = "https://open.larksuite.com"   # optional
//!
//! [standup]
//! lark_app = "main"   # resolves to the credentials above
//! ```
//!
//! `core` only defines the (de)serializable types; the host parses the table for
//! its `/api/lark-apps` endpoints, and each App resolves its reference inside its
//! own config loader (both already deserialize the full TOML).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default Lark base URL (international tenant). China tenants use
/// `https://open.feishu.cn`.
pub fn default_base_url() -> String {
    "https://open.larksuite.com".to_string()
}

/// One Lark application's credentials. Fields default to empty so a partially
/// filled `[lark-apps.<name>]` entry never fails the whole config parse — the
/// consumer building a client is what rejects empty credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkApp {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

impl Default for LarkApp {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            base_url: default_base_url(),
        }
    }
}

/// The console-managed set of Lark apps, keyed by name. Deserialized from the
/// `[lark-apps]` table of the console config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LarkRegistry {
    apps: HashMap<String, LarkApp>,
}

impl LarkRegistry {
    /// Look up an app by name.
    pub fn get(&self, name: &str) -> Option<&LarkApp> {
        self.apps.get(name)
    }

    /// Iterate `(name, app)` pairs — used by the host to render the registry.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &LarkApp)> {
        self.apps.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }
}
