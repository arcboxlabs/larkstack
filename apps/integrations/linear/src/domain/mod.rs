//! The normalized core — source-agnostic types that flow from the Linear
//! adapter through [`debounce`] to the Lark cards.

pub mod debounce;
pub mod reminders;

/// Issue priority, normalized from Linear's `0`–`4` scale.
pub enum Priority {
    None,
    Urgent,
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn from_linear(value: u8) -> Self {
        match value {
            1 => Self::Urgent,
            2 => Self::High,
            3 => Self::Medium,
            4 => Self::Low,
            _ => Self::None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Urgent => "Urgent",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::None => "None",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Urgent => "🔴",
            Self::High => "🟠",
            Self::Medium => "🟡",
            Self::Low => "🔵",
            Self::None => "⚪",
        }
    }

    pub fn display(&self) -> String {
        format!("{} {}", self.emoji(), self.label())
    }
}

/// A normalized issue create/update. Accumulated across a debounce window: a
/// create followed by updates stays a create, and change descriptions merge.
pub struct IssueNotification {
    pub is_create: bool,
    /// Whether any update in the window changed the issue's workflow state.
    /// Drives subscriber fan-out in the default "comments + status changes" mode.
    pub status_changed: bool,
    pub identifier: String,
    /// The Linear issue id (for follow-up GraphQL lookups, e.g. subscribers).
    pub issue_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Priority,
    pub assignee: Option<String>,
    pub url: String,
    pub changes: Vec<String>,
}
