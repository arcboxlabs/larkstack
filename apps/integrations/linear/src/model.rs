//! The Linear source's normalized output — what flows through debounce to cards.

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
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Priority,
    pub assignee: Option<String>,
    pub url: String,
    pub changes: Vec<String>,
}
