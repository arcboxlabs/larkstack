//! Linear issue/comment → Lark card builders.

use lark_kit::card::{LarkCard, LarkMessage, card, link_button, md_div, message};
use lark_kit::truncate;
use serde_json::{Value, json};

use crate::model::{IssueNotification, Priority};
use crate::source::models::LinearIssueData;

fn priority_color(priority: &Priority) -> &'static str {
    match priority {
        Priority::Urgent => "red",
        Priority::High => "orange",
        Priority::Medium => "yellow",
        _ => "blue",
    }
}

/// A "Status / Priority / Assignee" fields block.
fn fields(status: &str, priority: &str, assignee: Option<&str>) -> Value {
    let assignee = assignee.unwrap_or("Unassigned");
    json!({
        "tag": "div",
        "fields": [
            { "is_short": true, "text": { "tag": "lark_md", "content": format!("**Status:** {status}") } },
            { "is_short": true, "text": { "tag": "lark_md", "content": format!("**Priority:** {priority}") } },
            { "is_short": true, "text": { "tag": "lark_md", "content": format!("**Assignee:** {assignee}") } },
        ]
    })
}

fn view_button(url: &str) -> Value {
    link_button(url, "View in Linear")
}

/// Group-webhook card for an issue create/update.
pub fn issue_card(n: &IssueNotification) -> LarkMessage {
    let action = if n.is_create { "Created" } else { "Updated" };
    let mut elements = vec![md_div(&format!("**{}**", n.title))];

    // Description is shown on creates only.
    if n.is_create
        && let Some(desc) = &n.description
    {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            elements.push(md_div(&truncate(trimmed, 200)));
        }
    }

    if !n.changes.is_empty() {
        elements.push(md_div(&n.changes.join("\n")));
    }

    elements.push(fields(
        &n.status,
        &n.priority.display(),
        n.assignee.as_deref(),
    ));
    elements.push(view_button(&n.url));

    message(card(
        priority_color(&n.priority),
        format!("[Linear] {action}: {}", n.identifier),
        elements,
    ))
}

/// Group-webhook card for a new comment.
pub fn comment_card(
    identifier: &str,
    issue_title: &str,
    author: &str,
    body: &str,
    url: &str,
) -> LarkMessage {
    let issue_ref = if issue_title.is_empty() {
        "an issue".to_string()
    } else {
        format!("{identifier}: {issue_title}")
    };

    let mut elements = vec![md_div(&format!(
        "**{author}** commented on **{issue_ref}**"
    ))];
    let body = truncate(body.trim(), 200);
    if !body.is_empty() {
        elements.push(md_div(&body));
    }
    elements.push(view_button(url));

    message(card(
        "blue",
        format!("[Linear] Comment: {identifier}"),
        elements,
    ))
}

/// DM card notifying the assignee about an issue.
pub fn assign_dm(n: &IssueNotification) -> LarkCard {
    card(
        priority_color(&n.priority),
        format!("[Linear] Assigned: {}", n.identifier),
        vec![
            md_div(&format!(
                "You've been assigned to **{}**\n{}",
                n.identifier, n.title
            )),
            fields(&n.status, &n.priority.display(), None),
            view_button(&n.url),
        ],
    )
}

/// Inline link-preview card from GraphQL-fetched issue data.
pub fn preview_card(issue: &LinearIssueData) -> LarkCard {
    let priority = Priority::from_linear(issue.priority);
    let assignee = issue.assignee.as_ref().map(|a| a.name.as_str());

    let mut elements = vec![md_div(&format!("**{}**", issue.title))];
    if let Some(desc) = &issue.description {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            elements.push(md_div(&truncate(trimmed, 200)));
        }
    }
    elements.push(fields(&issue.state.name, &priority.display(), assignee));
    elements.push(view_button(&issue.url));

    card(
        priority_color(&priority),
        format!("[Linear] {}", issue.identifier),
        elements,
    )
}
