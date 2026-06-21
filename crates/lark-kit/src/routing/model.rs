use serde::{Deserialize, Serialize};

/// StateStore key (within the App's namespace) holding the routing [`Config`] JSON blob.
pub const KEY: &str = "routing";

/// A delivery target kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestKind {
    /// A Lark group chat, addressed by `chat_id`.
    Chat,
    /// A direct message to a user, addressed by their `open_id` (preferred — from the
    /// console user-picker) or by email (any target containing `@`).
    Dm,
}

/// One delivery target: a [`DestKind`] plus its address (`chat_id`, or a DM `open_id`/email).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    pub kind: DestKind,
    pub target: String,
}

impl Destination {
    /// A group-chat destination addressed by `chat_id`.
    pub fn chat(target: impl Into<String>) -> Self {
        Self {
            kind: DestKind::Chat,
            target: target.into(),
        }
    }

    /// A DM destination addressed by user `open_id` (or email).
    pub fn dm(target: impl Into<String>) -> Self {
        Self {
            kind: DestKind::Dm,
            target: target.into(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.target.trim().is_empty() {
            return Err("destination target must not be empty".into());
        }
        Ok(())
    }
}

/// A routing rule: deliver to `destinations` when the subject matches `r#match` and the
/// event is allowed by `events` (empty = all events).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Subject pattern: `"*"` (all), `"base/*"` (the `base` namespace and everything under
    /// it), or an exact path.
    #[serde(rename = "match")]
    pub match_: String,
    /// Event names this rule applies to (e.g. `merge_request`); empty = all events.
    #[serde(default)]
    pub events: Vec<String>,
    pub destinations: Vec<Destination>,
}

/// Maps a source username to a Lark email, for reviewer/assignee DMs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMap {
    pub username: String,
    pub lark_email: String,
}

/// The per-App console-editable notification config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub rules: Vec<Rule>,
    /// Destinations used when *no* rule matches the subject (empty = drop unmatched events).
    pub default_destinations: Vec<Destination>,
    /// Source-username → Lark-email map for reviewer/assignee DMs.
    pub user_map: Vec<UserMap>,
    /// Issue label titles (lowercased) that trigger an alert card.
    pub alert_labels: Vec<String>,
}

impl Config {
    /// The Lark email mapped to `username`, if any.
    pub fn lark_email(&self, username: &str) -> Option<&str> {
        self.user_map
            .iter()
            .find(|m| m.username == username)
            .map(|m| m.lark_email.as_str())
    }

    /// Whether `label` is configured as an alert label (case-insensitive on both sides, so
    /// it doesn't matter how the console stored them).
    pub fn is_alert_label(&self, label: &str) -> bool {
        let label = label.to_lowercase();
        self.alert_labels.iter().any(|l| l.to_lowercase() == label)
    }
}
