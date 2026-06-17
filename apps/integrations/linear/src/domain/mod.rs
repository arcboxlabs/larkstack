//! The normalized core — source-agnostic types that flow from the Linear
//! adapter through [`debounce`] to the Lark cards.

pub mod debounce;
pub mod reminders;

/// The routing *subject* for a Linear issue: its team key, taken from the
/// identifier prefix (`ENG-42` → `ENG`). Linear identifiers are always
/// `<teamKey>-<number>`, so this is uniform across issues and comments without
/// reaching into the payload's team object. Used to match `lark_kit::routing`
/// rules — an admin routes a team's notifications by its key. Falls back to the
/// whole identifier when there's no `-` (e.g. an unknown `"?"`).
pub fn team_key(identifier: &str) -> &str {
    identifier.split('-').next().unwrap_or(identifier)
}

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

#[cfg(test)]
mod tests {
    use super::team_key;

    #[test]
    fn team_key_is_the_identifier_prefix() {
        assert_eq!(team_key("ENG-42"), "ENG");
        assert_eq!(team_key("PROD-1234"), "PROD");
        // No dash (e.g. an unknown comment identifier) → the whole string.
        assert_eq!(team_key("?"), "?");
        assert_eq!(team_key(""), "");
        // Only the first segment matters even with extra dashes.
        assert_eq!(team_key("ENG-42-x"), "ENG");
    }
}
