//! Console-configurable notification routing, shared by the source integrations.
//!
//! An integration (github, gitlab, …) maps a *subject* — a project/repo path such as
//! `group/project` or `owner/repo` — and an *event* string to one or more Lark
//! [`Destination`]s (a group chat by `chat_id`, or a DM by user `open_id`/email). The ruleset is a single
//! JSON blob in the per-App [`StateStore`] (key [`KEY`]), edited live from the console and
//! [loaded](Config::load) fresh on every webhook, so changes apply without a restart.
//!
//! The matcher and model are source-agnostic; each app supplies its own subject and event
//! vocabulary. Card delivery goes through the Lark bot ([`deliver`]).

mod admin;

pub use admin::RoutingApi;

use std::sync::Arc;

use larkstack_core::StateStore;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::LarkBotClient;
use crate::card::LarkCard;

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

    fn validate(&self) -> Result<(), String> {
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
    /// Load the config from `store` under `namespace`, falling back to defaults when absent
    /// or unparseable (the integration degrades to "no routing" rather than failing).
    pub async fn load(store: &Arc<dyn StateStore>, namespace: &str) -> Config {
        match store.get(namespace, KEY).await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!("routing config parse failed for '{namespace}', using empty: {e}");
                Config::default()
            }),
            Ok(None) => Config::default(),
            Err(e) => {
                warn!("routing config load failed for '{namespace}', using empty: {e}");
                Config::default()
            }
        }
    }

    /// Persist the (already-[validated](Config::validate)) config.
    pub async fn save(
        store: &Arc<dyn StateStore>,
        namespace: &str,
        config: &Config,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(config)?;
        store.set(namespace, KEY, &json).await
    }

    /// Reject structurally invalid configs (empty match/target) before saving.
    pub fn validate(&self) -> Result<(), String> {
        for rule in &self.rules {
            if rule.match_.trim().is_empty() {
                return Err("rule match must not be empty".into());
            }
            for d in &rule.destinations {
                d.validate()?;
            }
        }
        for d in &self.default_destinations {
            d.validate()?;
        }
        Ok(())
    }

    /// The deduplicated destinations for `(subject, event)`.
    ///
    /// Unions the destinations of every rule whose pattern matches `subject` and whose
    /// `events` filter allows `event`. If no rule matches the subject at all, falls back to
    /// [`default_destinations`](Config::default_destinations); a subject that matches a rule
    /// but not this event yields nothing (the project is configured, this event isn't routed).
    pub fn destinations_for(&self, subject: &str, event: &str) -> Vec<Destination> {
        let mut out: Vec<Destination> = Vec::new();
        let mut subject_matched = false;
        for rule in &self.rules {
            if !matches(&rule.match_, subject) {
                continue;
            }
            subject_matched = true;
            if !rule.events.is_empty() && !rule.events.iter().any(|e| e == event) {
                continue;
            }
            for d in &rule.destinations {
                push_unique(&mut out, d);
            }
        }
        if !subject_matched {
            for d in &self.default_destinations {
                push_unique(&mut out, d);
            }
        }
        out
    }

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

fn push_unique(out: &mut Vec<Destination>, d: &Destination) {
    if !out.iter().any(|o| o.kind == d.kind && o.target == d.target) {
        out.push(d.clone());
    }
}

/// Matches a subject path against a rule pattern: `"*"` = any; `"base/*"` = `base` or
/// anything under `base/`; otherwise an exact match.
fn matches(pattern: &str, subject: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(base) = pattern.strip_suffix("/*") {
        return subject == base || subject.starts_with(&format!("{base}/"));
    }
    pattern == subject
}

/// Deliver `card` to a single [`Destination`] via the bot. Logs and skips when no bot is
/// configured or the send fails (delivery is best-effort, like the group-webhook sender).
pub async fn deliver(bot: Option<&LarkBotClient>, dest: &Destination, card: &LarkCard) {
    let Some(bot) = bot else {
        warn!(
            "routing: no Lark bot configured — cannot deliver to {:?} {}",
            dest.kind, dest.target
        );
        return;
    };
    let res = match dest.kind {
        DestKind::Chat => bot.reply_to_chat(&dest.target, card).await,
        // DM targets are user `open_id`s (from the console picker); a target that looks
        // like an email (`@`) is still delivered by email for back-compat / manual entry.
        DestKind::Dm if dest.target.contains('@') => bot.send_dm(&dest.target, card).await,
        DestKind::Dm => bot.send_dm_by_open_id(&dest.target, card).await,
    };
    if let Err(e) = res {
        warn!(
            "routing: failed to deliver to {:?} {}: {e}",
            dest.kind, dest.target
        );
    }
}

/// Deliver `card` to every destination in turn.
pub async fn deliver_all(bot: Option<&LarkBotClient>, dests: &[Destination], card: &LarkCard) {
    for d in dests {
        deliver(bot, d, card).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str) -> Destination {
        Destination {
            kind: DestKind::Chat,
            target: id.into(),
        }
    }
    fn dm(email: &str) -> Destination {
        Destination {
            kind: DestKind::Dm,
            target: email.into(),
        }
    }
    fn rule(m: &str, events: &[&str], dests: Vec<Destination>) -> Rule {
        Rule {
            match_: m.into(),
            events: events.iter().map(|s| s.to_string()).collect(),
            destinations: dests,
        }
    }

    #[test]
    fn matches_wildcard_prefix_and_exact() {
        assert!(matches("*", "any/thing"));
        assert!(matches("grp/*", "grp"));
        assert!(matches("grp/*", "grp/proj"));
        assert!(matches("grp/*", "grp/sub/proj"));
        assert!(!matches("grp/*", "grpx"));
        assert!(!matches("grp/*", "other/proj"));
        assert!(matches("grp/proj", "grp/proj"));
        assert!(!matches("grp/proj", "grp/other"));
    }

    #[test]
    fn destinations_union_and_dedup() {
        let cfg = Config {
            rules: vec![
                rule("grp/*", &[], vec![chat("c1"), dm("a@x")]),
                rule("grp/proj", &["merge_request"], vec![chat("c1"), chat("c2")]),
            ],
            ..Default::default()
        };
        let got = cfg.destinations_for("grp/proj", "merge_request");
        // c1 deduped across rules; order = first-seen.
        assert_eq!(got, vec![chat("c1"), dm("a@x"), chat("c2")]);
    }

    #[test]
    fn event_filter_excludes_but_does_not_fall_back_to_default() {
        let cfg = Config {
            rules: vec![rule("grp/proj", &["pipeline"], vec![chat("c1")])],
            default_destinations: vec![dm("fallback@x")],
            ..Default::default()
        };
        // subject matches the rule, event doesn't → no destinations, no default.
        assert!(cfg.destinations_for("grp/proj", "issue").is_empty());
    }

    #[test]
    fn default_used_only_when_no_rule_matches_subject() {
        let cfg = Config {
            rules: vec![rule("grp/*", &[], vec![chat("c1")])],
            default_destinations: vec![dm("fallback@x")],
            ..Default::default()
        };
        assert_eq!(
            cfg.destinations_for("other/proj", "issue"),
            vec![dm("fallback@x")]
        );
        assert_eq!(cfg.destinations_for("grp/proj", "issue"), vec![chat("c1")]);
    }

    #[test]
    fn tolerant_decode_and_lookups() {
        // Missing fields decode to defaults.
        let cfg: Config = serde_json::from_str("{}").unwrap();
        assert!(cfg.rules.is_empty());
        // Partial blob with only user_map + alert_labels.
        let cfg: Config = serde_json::from_str(
            r#"{"user_map":[{"username":"octo","lark_email":"o@x"}],"alert_labels":["bug","P0"]}"#,
        )
        .unwrap();
        assert_eq!(cfg.lark_email("octo"), Some("o@x"));
        assert_eq!(cfg.lark_email("nobody"), None);
        assert!(cfg.is_alert_label("BUG"));
        assert!(cfg.is_alert_label("p0"));
        assert!(!cfg.is_alert_label("wontfix"));
    }

    #[test]
    fn validate_rejects_empty_target_and_match() {
        let bad_target = Config {
            rules: vec![rule("grp/*", &[], vec![chat("")])],
            ..Default::default()
        };
        assert!(bad_target.validate().is_err());
        let bad_match = Config {
            rules: vec![rule("  ", &[], vec![chat("c1")])],
            ..Default::default()
        };
        assert!(bad_match.validate().is_err());
        let ok = Config {
            rules: vec![rule("grp/*", &[], vec![chat("c1")])],
            ..Default::default()
        };
        assert!(ok.validate().is_ok());
    }
}
