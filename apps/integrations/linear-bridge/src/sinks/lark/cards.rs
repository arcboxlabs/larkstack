//! Converts [`Event`]s and [`LinearIssueData`] into Lark interactive cards.

use serde_json::{Value, json};

use crate::{
    event::{Event, Priority},
    sources::{linear::models::LinearIssueData, x::TweetData},
    utils::truncate,
};

use super::models::{LarkCard, LarkHeader, LarkMessage, LarkTitle};

/// Returns the Lark header color template for a given priority.
fn priority_color(priority: &Priority) -> &'static str {
    match priority {
        Priority::Urgent => "red",
        Priority::High => "orange",
        Priority::Medium => "yellow",
        _ => "blue",
    }
}

/// Builds a "Status / Priority / Assignee" fields block.
fn build_fields(status: &str, priority: &str, assignee: Option<&str>) -> Value {
    let assignee = assignee.unwrap_or("Unassigned");
    let fields = vec![
        json!({
            "is_short": true,
            "text": {
                "tag": "lark_md",
                "content": format!("**Status:** {status}"),
            }
        }),
        json!({
            "is_short": true,
            "text": {
                "tag": "lark_md",
                "content": format!("**Priority:** {priority}"),
            }
        }),
        json!({
            "is_short": true,
            "text": {
                "tag": "lark_md",
                "content": format!("**Assignee:** {assignee}"),
            }
        }),
    ];
    json!({ "tag": "div", "fields": fields })
}

/// Builds a "View in Linear" action button element.
fn build_action_button(url: &str) -> Value {
    build_link_button(url, "View in Linear")
}

/// Builds a single-button action row linking to `url` with a custom label.
fn build_link_button(url: &str, label: &str) -> Value {
    json!({
        "tag": "action",
        "actions": [{
            "tag": "button",
            "text": { "tag": "plain_text", "content": label },
            "type": "primary",
            "url": url,
        }]
    })
}

/// A `lark_md` text div element.
fn md_div(content: &str) -> Value {
    json!({
        "tag": "div",
        "text": { "tag": "lark_md", "content": content },
    })
}

/// Wraps `elements` in an interactive [`LarkMessage`] with a colored header.
fn build_card(color: &str, header_text: String, elements: Vec<Value>) -> LarkMessage {
    LarkMessage {
        msg_type: "interactive",
        card: LarkCard {
            config: None,
            header: LarkHeader {
                template: color.to_string(),
                title: LarkTitle {
                    content: header_text,
                    tag: "plain_text",
                },
            },
            elements,
        },
    }
}

/// Formats an [`Event`] as a [`LarkMessage`] for group webhook delivery.
pub fn build_lark_card(event: &Event) -> LarkMessage {
    match event {
        Event::IssueCreated {
            identifier,
            title,
            description,
            status,
            priority,
            assignee,
            url,
            changes,
            ..
        } => build_issue_card(
            "Created",
            identifier,
            title,
            description.as_deref(),
            status,
            priority,
            assignee.as_deref(),
            url,
            changes,
        ),
        Event::IssueUpdated {
            identifier,
            title,
            status,
            priority,
            assignee,
            url,
            changes,
            ..
        } => build_issue_card(
            "Updated",
            identifier,
            title,
            None,
            status,
            priority,
            assignee.as_deref(),
            url,
            changes,
        ),
        Event::CommentCreated {
            identifier,
            issue_title,
            author,
            body,
            url,
            ..
        } => build_comment_card(identifier, issue_title, author, body, url),

        // --- GitHub events ---
        Event::PrOpened {
            repo,
            number,
            title,
            author,
            head_branch,
            base_branch,
            additions,
            deletions,
            url,
        } => build_pr_opened_card(
            repo,
            *number,
            title,
            author,
            head_branch,
            base_branch,
            *additions,
            *deletions,
            url,
        ),
        Event::PrReviewRequested {
            repo,
            number,
            title,
            author,
            reviewer,
            reviewer_lark_id,
            url,
        } => build_pr_review_requested_card(
            repo,
            *number,
            title,
            author,
            reviewer,
            reviewer_lark_id.as_deref(),
            url,
        ),
        Event::PrMerged {
            repo,
            number,
            title,
            author,
            merged_by,
            url,
        } => build_pr_merged_card(repo, *number, title, author, merged_by, url),
        Event::IssueLabeledAlert {
            repo,
            number,
            title,
            label,
            author,
            url,
        } => build_issue_labeled_card(repo, *number, title, label, author, url),
        Event::WorkflowRunFailed {
            repo,
            workflow_name,
            branch,
            actor,
            url,
            ..
        } => build_workflow_failed_card(repo, workflow_name, branch, actor, url),
        Event::SecretScanningAlert {
            repo,
            secret_type,
            url,
        } => build_secret_scanning_card(repo, secret_type, url),
        Event::DependabotAlert {
            repo,
            package,
            severity,
            summary,
            url,
        } => build_dependabot_card(repo, package, severity, summary, url),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_issue_card(
    action: &str,
    identifier: &str,
    title: &str,
    description: Option<&str>,
    status: &str,
    priority: &Priority,
    assignee: Option<&str>,
    url: &str,
    changes: &[String],
) -> LarkMessage {
    let color = priority_color(priority);
    let assignee_name = assignee.unwrap_or("Unassigned");

    let mut elements = vec![];

    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{title}**"),
        }
    }));

    if let Some(desc) = description {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            elements.push(json!({
                "tag": "div",
                "text": {
                    "tag": "lark_md",
                    "content": truncate(trimmed, 200),
                }
            }));
        }
    }

    if !changes.is_empty() {
        let change_text = changes.join("\n");
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": change_text,
            }
        }));
    }

    elements.push(build_fields(
        status,
        &priority.display(),
        Some(assignee_name),
    ));
    elements.push(build_action_button(url));

    LarkMessage {
        msg_type: "interactive",
        card: LarkCard {
            config: None,
            header: LarkHeader {
                template: color.to_string(),
                title: LarkTitle {
                    content: format!("[Linear] {action}: {identifier}"),
                    tag: "plain_text",
                },
            },
            elements,
        },
    }
}

fn build_comment_card(
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

    let mut elements = vec![];

    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{author}** commented on **{issue_ref}**"),
        }
    }));

    let body = truncate(body.trim(), 200);
    if !body.is_empty() {
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": body,
            }
        }));
    }

    elements.push(build_action_button(url));

    LarkMessage {
        msg_type: "interactive",
        card: LarkCard {
            config: None,
            header: LarkHeader {
                template: "blue".to_string(),
                title: LarkTitle {
                    content: format!("[Linear] Comment: {identifier}"),
                    tag: "plain_text",
                },
            },
            elements,
        },
    }
}

/// Builds a DM card for assignment (Linear) or review-request (GitHub)
/// notifications. Returns `None` for events that don't warrant a DM.
pub fn build_assign_dm_card(event: &Event) -> Option<LarkCard> {
    match event {
        Event::IssueCreated {
            identifier,
            title,
            status,
            priority,
            url,
            ..
        }
        | Event::IssueUpdated {
            identifier,
            title,
            status,
            priority,
            url,
            ..
        } => {
            let elements = vec![
                md_div(&format!(
                    "You've been assigned to **{identifier}**\n{title}"
                )),
                build_fields(status, &priority.display(), None),
                build_action_button(url),
            ];
            Some(LarkCard {
                config: None,
                header: LarkHeader {
                    template: priority_color(priority).to_string(),
                    title: LarkTitle {
                        content: format!("[Linear] Assigned: {identifier}"),
                        tag: "plain_text",
                    },
                },
                elements,
            })
        }
        Event::PrReviewRequested {
            repo,
            number,
            title,
            author,
            url,
            ..
        } => {
            let elements = vec![
                md_div(&format!(
                    "**{author}** requested your review on **#{number}**\n{title}"
                )),
                md_div(&format!("**Repository:** {repo}")),
                build_link_button(url, "View on GitHub"),
            ];
            Some(LarkCard {
                config: None,
                header: LarkHeader {
                    template: "yellow".to_string(),
                    title: LarkTitle {
                        content: format!("[{repo}] Review Requested #{number}"),
                        tag: "plain_text",
                    },
                },
                elements,
            })
        }
        _ => None,
    }
}

/// Builds an inline preview card from GraphQL-fetched issue data.
///
/// This is used for Lark link unfurling and does **not** go through [`Event`].
pub fn build_preview_card(issue: &LinearIssueData) -> LarkCard {
    let priority = Priority::from_linear(issue.priority);
    let color = priority_color(&priority);
    let assignee = issue
        .assignee
        .as_ref()
        .map(|a| a.name.as_str())
        .unwrap_or("Unassigned");

    let mut elements = vec![];

    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{}**", issue.title),
        }
    }));

    if let Some(desc) = &issue.description {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            elements.push(json!({
                "tag": "div",
                "text": {
                    "tag": "lark_md",
                    "content": truncate(trimmed, 200),
                }
            }));
        }
    }

    elements.push(build_fields(
        &issue.state.name,
        &priority.display(),
        Some(assignee),
    ));
    elements.push(build_action_button(&issue.url));

    LarkCard {
        config: None,
        header: LarkHeader {
            template: color.to_string(),
            title: LarkTitle {
                content: format!("[Linear] {}", issue.identifier),
                tag: "plain_text",
            },
        },
        elements,
    }
}

// ---------------------------------------------------------------------------
// GitHub card builders (private)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_pr_opened_card(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    head_branch: &str,
    base_branch: &str,
    additions: u64,
    deletions: u64,
    url: &str,
) -> LarkMessage {
    let elements = vec![
        md_div(&format!("**{title}**")),
        md_div(&format!(
            "**Branch:** `{head_branch}` → `{base_branch}`\n**Changes:** +{additions} / -{deletions}"
        )),
        md_div(&format!("**Author:** {author}")),
        build_link_button(url, "View on GitHub"),
    ];
    build_card("purple", format!("[{repo}] PR Opened #{number}"), elements)
}

fn build_pr_review_requested_card(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    reviewer: &str,
    reviewer_lark_id: Option<&str>,
    url: &str,
) -> LarkMessage {
    let reviewer_display = match reviewer_lark_id {
        Some(email) => format!("<at email={email}></at>"),
        None => reviewer.to_string(),
    };
    let elements = vec![
        md_div(&format!("**{title}**")),
        md_div(&format!(
            "**Reviewer:** {reviewer_display}\n**Author:** {author}"
        )),
        build_link_button(url, "View on GitHub"),
    ];
    build_card(
        "yellow",
        format!("[{repo}] Review Requested #{number}"),
        elements,
    )
}

fn build_pr_merged_card(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    merged_by: &str,
    url: &str,
) -> LarkMessage {
    let elements = vec![
        md_div(&format!("**{title}**")),
        md_div(&format!("**Merged by:** {merged_by}\n**Author:** {author}")),
        build_link_button(url, "View on GitHub"),
    ];
    build_card("green", format!("[{repo}] PR Merged #{number}"), elements)
}

fn build_issue_labeled_card(
    repo: &str,
    number: u64,
    title: &str,
    label: &str,
    author: &str,
    url: &str,
) -> LarkMessage {
    let elements = vec![
        md_div(&format!("**{title}**")),
        md_div(&format!("**Label:** `{label}`\n**Author:** {author}")),
        build_link_button(url, "View on GitHub"),
    ];
    build_card("red", format!("[{repo}] Issue Alert #{number}"), elements)
}

fn build_workflow_failed_card(
    repo: &str,
    workflow_name: &str,
    branch: &str,
    actor: &str,
    url: &str,
) -> LarkMessage {
    let elements = vec![
        md_div(&format!("**Workflow:** {workflow_name}")),
        md_div(&format!(
            "**Branch:** `{branch}`\n**Triggered by:** {actor}"
        )),
        build_link_button(url, "View Workflow Run"),
    ];
    build_card("red", format!("[{repo}] CI Failed"), elements)
}

fn build_secret_scanning_card(repo: &str, secret_type: &str, url: &str) -> LarkMessage {
    let elements = vec![
        md_div(&format!(
            "**Secret type:** {secret_type}\n\nA leaked credential was detected in the repository. Rotate this secret immediately."
        )),
        build_link_button(url, "View Alert"),
    ];
    build_card("red", format!("[{repo}] Secret Leaked"), elements)
}

fn build_dependabot_card(
    repo: &str,
    package: &str,
    severity: &str,
    summary: &str,
    url: &str,
) -> LarkMessage {
    let color = if severity == "critical" {
        "red"
    } else {
        "orange"
    };
    let elements = vec![
        md_div(&format!(
            "**Package:** `{package}`\n**Severity:** {severity}"
        )),
        md_div(summary),
        build_link_button(url, "View Alert"),
    ];
    build_card(color, format!("[{repo}] Dependabot Alert"), elements)
}

// ---------------------------------------------------------------------------
// X (Twitter) preview card
// ---------------------------------------------------------------------------

/// Builds an inline preview card from fetched tweet data. Returns
/// `(card, inline_title)` — the inline title is the short text shown in chat
/// before the card expands. Does **not** go through [`Event`].
pub fn build_x_preview_card(tweet: &TweetData) -> (LarkCard, String) {
    let author_at = if tweet.author_username.is_empty() {
        tweet.author_name.clone()
    } else {
        format!("@{}", tweet.author_username)
    };

    let mut elements = vec![];
    if !tweet.text.is_empty() {
        elements.push(json!({
            "tag": "markdown",
            "content": truncate(&tweet.text, 200),
        }));
    }

    let note = if tweet.like_count.is_some() || tweet.retweet_count.is_some() {
        let likes = tweet.like_count.unwrap_or(0);
        let retweets = tweet.retweet_count.unwrap_or(0);
        format!("❤️ {likes}  🔁 {retweets}  •  {author_at} on X")
    } else if !author_at.is_empty() {
        format!("From {author_at} on X")
    } else {
        String::new()
    };
    if !note.is_empty() {
        elements.push(md_div(&note));
    }
    elements.push(build_link_button(&tweet.url, "View on X"));

    let header_title = if tweet.author_name.is_empty() {
        "X Post".to_string()
    } else {
        tweet.author_name.clone()
    };
    let card = LarkCard {
        config: None,
        header: LarkHeader {
            template: "blue".to_string(),
            title: LarkTitle {
                content: header_title,
                tag: "plain_text",
            },
        },
        elements,
    };

    let inline_title = if !author_at.is_empty() && !tweet.text.is_empty() {
        format!("{}: {}...", author_at, truncate(&tweet.text, 30))
    } else if !author_at.is_empty() {
        format!("Post by {author_at}")
    } else {
        "X Post".to_string()
    };

    (card, inline_title)
}
