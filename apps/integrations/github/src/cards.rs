//! GitHub event → Lark card builders.
//!
//! All builders return a bare [`LarkCard`] — delivery (group chat or DM) is the caller's
//! concern, via [`lark_kit::routing`].

use lark_kit::card::{LarkCard, card, link_button, md_div};

#[allow(clippy::too_many_arguments)]
pub fn pr_opened(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    head_branch: &str,
    base_branch: &str,
    additions: u64,
    deletions: u64,
    url: &str,
) -> LarkCard {
    card(
        "purple",
        format!("[{repo}] PR Opened #{number}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!(
                "**Branch:** `{head_branch}` → `{base_branch}`\n**Changes:** +{additions} / -{deletions}"
            )),
            md_div(&format!("**Author:** {author}")),
            link_button(url, "View on GitHub"),
        ],
    )
}

pub fn pr_review_requested(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    reviewer: &str,
    reviewer_lark_id: Option<&str>,
    url: &str,
) -> LarkCard {
    let reviewer_display = match reviewer_lark_id {
        Some(email) => format!("<at email={email}></at>"),
        None => reviewer.to_string(),
    };
    card(
        "yellow",
        format!("[{repo}] Review Requested #{number}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!(
                "**Reviewer:** {reviewer_display}\n**Author:** {author}"
            )),
            link_button(url, "View on GitHub"),
        ],
    )
}

/// DM card for the requested reviewer.
pub fn pr_review_dm(repo: &str, number: u64, title: &str, author: &str, url: &str) -> LarkCard {
    card(
        "yellow",
        format!("[{repo}] Review Requested #{number}"),
        vec![
            md_div(&format!(
                "**{author}** requested your review on **#{number}**\n{title}"
            )),
            md_div(&format!("**Repository:** {repo}")),
            link_button(url, "View on GitHub"),
        ],
    )
}

pub fn pr_merged(
    repo: &str,
    number: u64,
    title: &str,
    author: &str,
    merged_by: &str,
    url: &str,
) -> LarkCard {
    card(
        "green",
        format!("[{repo}] PR Merged #{number}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!("**Merged by:** {merged_by}\n**Author:** {author}")),
            link_button(url, "View on GitHub"),
        ],
    )
}

pub fn issue_labeled(
    repo: &str,
    number: u64,
    title: &str,
    label: &str,
    author: &str,
    url: &str,
) -> LarkCard {
    card(
        "red",
        format!("[{repo}] Issue Alert #{number}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!("**Label:** `{label}`\n**Author:** {author}")),
            link_button(url, "View on GitHub"),
        ],
    )
}

pub fn workflow_failed(
    repo: &str,
    workflow_name: &str,
    branch: &str,
    actor: &str,
    url: &str,
) -> LarkCard {
    card(
        "red",
        format!("[{repo}] CI Failed"),
        vec![
            md_div(&format!("**Workflow:** {workflow_name}")),
            md_div(&format!(
                "**Branch:** `{branch}`\n**Triggered by:** {actor}"
            )),
            link_button(url, "View Workflow Run"),
        ],
    )
}

pub fn secret_scanning(repo: &str, secret_type: &str, url: &str) -> LarkCard {
    card(
        "red",
        format!("[{repo}] Secret Leaked"),
        vec![
            md_div(&format!(
                "**Secret type:** {secret_type}\n\nA leaked credential was detected in the repository. Rotate this secret immediately."
            )),
            link_button(url, "View Alert"),
        ],
    )
}

pub fn dependabot(repo: &str, package: &str, severity: &str, summary: &str, url: &str) -> LarkCard {
    let color = if severity == "critical" {
        "red"
    } else {
        "orange"
    };
    card(
        color,
        format!("[{repo}] Dependabot Alert"),
        vec![
            md_div(&format!(
                "**Package:** `{package}`\n**Severity:** {severity}"
            )),
            md_div(summary),
            link_button(url, "View Alert"),
        ],
    )
}
