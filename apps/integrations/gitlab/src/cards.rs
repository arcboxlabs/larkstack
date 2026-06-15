//! GitLab event → Lark card builders.
//!
//! `repo` is the project's `path_with_namespace` (e.g. `group/project`), used in
//! every card header.

use lark_kit::card::{LarkCard, LarkMessage, card, link_button, md_div, message};

pub fn mr_opened(
    repo: &str,
    iid: u64,
    title: &str,
    author: &str,
    source_branch: &str,
    target_branch: &str,
    url: &str,
) -> LarkMessage {
    message(card(
        "purple",
        format!("[{repo}] MR Opened !{iid}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!(
                "**Branch:** `{source_branch}` → `{target_branch}`"
            )),
            md_div(&format!("**Author:** {author}")),
            link_button(url, "View in GitLab"),
        ],
    ))
}

pub fn mr_merged(repo: &str, iid: u64, title: &str, author: &str, url: &str) -> LarkMessage {
    message(card(
        "green",
        format!("[{repo}] MR Merged !{iid}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!("**Author:** {author}")),
            link_button(url, "View in GitLab"),
        ],
    ))
}

/// DM card for a requested reviewer / assignee.
pub fn mr_review_dm(repo: &str, iid: u64, title: &str, author: &str, url: &str) -> LarkCard {
    card(
        "yellow",
        format!("[{repo}] Review Requested !{iid}"),
        vec![
            md_div(&format!(
                "**{author}** requested your review on **!{iid}**\n{title}"
            )),
            md_div(&format!("**Project:** {repo}")),
            link_button(url, "View in GitLab"),
        ],
    )
}

pub fn note(repo: &str, noteable: &str, author: &str, snippet: &str, url: &str) -> LarkMessage {
    message(card(
        "blue",
        format!("[{repo}] Comment on {noteable}"),
        vec![
            md_div(&format!("**Author:** {author}")),
            md_div(snippet),
            link_button(url, "View in GitLab"),
        ],
    ))
}

pub fn issue_labeled(
    repo: &str,
    iid: u64,
    title: &str,
    label: &str,
    author: &str,
    url: &str,
) -> LarkMessage {
    message(card(
        "red",
        format!("[{repo}] Issue Alert #{iid}"),
        vec![
            md_div(&format!("**{title}**")),
            md_div(&format!("**Label:** `{label}`\n**Author:** {author}")),
            link_button(url, "View in GitLab"),
        ],
    ))
}

pub fn pipeline_failed(
    repo: &str,
    ref_name: &str,
    author: &str,
    commit_title: Option<&str>,
    url: &str,
) -> LarkMessage {
    let mut elements = vec![md_div(&format!(
        "**Branch:** `{ref_name}`\n**Triggered by:** {author}"
    ))];
    if let Some(commit) = commit_title {
        elements.push(md_div(&format!("**Commit:** {commit}")));
    }
    elements.push(link_button(url, "View Pipeline"));
    message(card("red", format!("[{repo}] Pipeline Failed"), elements))
}

pub fn push(
    repo: &str,
    ref_name: &str,
    pusher: &str,
    total: u64,
    commits: &[crate::source::payload::Commit],
) -> LarkMessage {
    let plural = if total == 1 { "commit" } else { "commits" };
    let mut elements = vec![md_div(&format!(
        "**Branch:** `{ref_name}`\n**Pushed by:** {pusher} ({total} {plural})"
    ))];
    let lines: Vec<String> = commits
        .iter()
        .take(5)
        .map(|c| format!("- [{}]({})", c.title, c.url))
        .collect();
    if !lines.is_empty() {
        elements.push(md_div(&lines.join("\n")));
    }
    message(card("blue", format!("[{repo}] New Push"), elements))
}
