//! Change detection — diffs an updated issue against its previous state to
//! produce human-readable change lines for the notification card.

use super::payload::{Issue, UpdatedFrom};
use crate::domain::Priority;

/// Compares the current [`Issue`] state against `updated_from` and returns
/// human-readable change descriptions (e.g. `"**Status:** Todo → In Progress"`).
pub fn build_change_fields(issue: &Issue, updated_from: &Option<serde_json::Value>) -> Vec<String> {
    let mut changes = Vec::new();

    let Some(uf_value) = updated_from else {
        return changes;
    };

    let Ok(uf) = serde_json::from_value::<UpdatedFrom>(uf_value.clone()) else {
        return changes;
    };

    if let Some(old_state) = &uf.state {
        let old_name = old_state
            .get("name")
            .and_then(|v| v.as_str())
            // Linear sometimes sends state as a flat string
            .or_else(|| old_state.as_str())
            .unwrap_or("Unknown");
        changes.push(format!("**Status:** {} → {}", old_name, issue.state.name));
    }

    if let Some(old_priority) = uf.priority {
        changes.push(format!(
            "**Priority:** {} → {}",
            Priority::from_linear(old_priority).display(),
            Priority::from_linear(issue.priority).display()
        ));
    }

    if uf.assignee_id.is_some() || uf.assignee.is_some() {
        let old_name = uf
            .assignee
            .as_ref()
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unassigned");
        let new_name = issue
            .assignee
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("Unassigned");
        changes.push(format!("**Assignee:** {} → {}", old_name, new_name));
    }

    changes
}
