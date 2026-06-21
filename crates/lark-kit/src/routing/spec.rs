use std::collections::BTreeSet;

use serde::Serialize;

/// The static routing capabilities one App exposes to the console.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RoutingSpec {
    /// StateStore namespace for this App's routing config. Usually the App name.
    pub namespace: &'static str,
    /// Human-facing description of what a rule's subject matches.
    pub subject: SubjectSpec,
    /// Event vocabulary accepted in rule filters.
    pub events: &'static [RoutingEvent],
    /// Optional routing-adjacent UI/policy features supported by this App.
    pub features: RoutingFeatures,
}

impl RoutingSpec {
    pub(super) fn event_values(&self) -> BTreeSet<&'static str> {
        self.events.iter().map(|event| event.value).collect()
    }
}

/// Human-facing metadata for the routing subject.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SubjectSpec {
    /// Short label, e.g. "Repository" or "Team key".
    pub label: &'static str,
    /// Example value for empty rule fields.
    pub placeholder: &'static str,
    /// Help text explaining how the App derives the subject.
    pub help: &'static str,
}

/// One App-specific event value users can route.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RoutingEvent {
    /// Stable wire value used in [`crate::routing::Rule::events`].
    pub value: &'static str,
    /// Human-facing label.
    pub label: &'static str,
    /// Short description for UI help.
    pub description: &'static str,
}

/// Optional routing-adjacent features the shared console editor can render.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RoutingFeatures {
    /// Source user → Lark email map for reviewer/assignee DMs.
    pub user_map: bool,
    /// Label list used by source integrations to decide alert-worthy issues.
    pub alert_labels: bool,
    /// Whether the chat picker should be offered.
    pub chat_picker: bool,
    /// Whether the DM user picker should be offered.
    pub user_picker: bool,
}

impl RoutingFeatures {
    /// The common bot-backed feature set for GitHub/GitLab-style integrations.
    pub const SOURCE_WITH_ALERTS: Self = Self {
        user_map: true,
        alert_labels: true,
        chat_picker: true,
        user_picker: true,
    };

    /// Bot-backed routing only; app-specific policy fields are hidden/rejected.
    pub const ROUTING_ONLY: Self = Self {
        user_map: false,
        alert_labels: false,
        chat_picker: true,
        user_picker: true,
    };
}
