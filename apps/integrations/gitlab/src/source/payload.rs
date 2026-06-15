//! Deserialization types for GitLab webhook payloads.
//!
//! The `gitlab` crate dropped its typed webhook structs (0.1900.1; upstream now
//! says "parse hooks yourself"), so — like the linear app — we own the minimal
//! subset the cards need. GitLab nests the event-specific data under
//! `object_attributes`, with `user` / `project` (and sometimes `assignees` /
//! `reviewers` / `labels` / `commit`) at the root.

use serde::Deserialize;

/// A GitLab user object, as embedded in webhook payloads.
#[derive(Debug, Deserialize)]
pub struct User {
    pub username: String,
    pub name: String,
}

/// The `project` block, present on every webhook.
#[derive(Debug, Deserialize)]
pub struct Project {
    pub path_with_namespace: String,
    pub web_url: String,
}

/// A GitLab label — note the name lives in `title`, not `name`.
#[derive(Debug, Deserialize)]
pub struct Label {
    pub title: String,
}

/// A commit, as embedded in pipeline and push payloads.
#[derive(Debug, Deserialize)]
pub struct Commit {
    pub title: String,
    pub url: String,
}

/// Minimal probe parsed before the full event, to dispatch on the discriminator.
#[derive(Debug, Deserialize)]
pub struct KindProbe {
    pub object_kind: String,
}

// ---- Merge Request (`object_kind: "merge_request"`) ----

#[derive(Debug, Deserialize)]
pub struct MergeRequestEvent {
    pub project: Project,
    pub user: User,
    pub object_attributes: MrAttrs,
    #[serde(default)]
    pub assignees: Vec<User>,
    #[serde(default)]
    pub reviewers: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub struct MrAttrs {
    pub iid: u64,
    pub title: String,
    pub state: String,
    pub action: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    pub url: String,
}

// ---- Issue (`object_kind: "issue"`) ----

#[derive(Debug, Deserialize)]
pub struct IssueEvent {
    pub project: Project,
    pub user: User,
    pub object_attributes: IssueAttrs,
    /// Field-level diff GitLab includes on updates; used to detect *newly added*
    /// labels so an alert fires once, not on every subsequent issue edit.
    #[serde(default)]
    pub changes: Option<IssueChanges>,
}

#[derive(Debug, Deserialize)]
pub struct IssueAttrs {
    pub iid: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct IssueChanges {
    #[serde(default)]
    pub labels: Option<LabelChange>,
}

#[derive(Debug, Deserialize)]
pub struct LabelChange {
    #[serde(default)]
    pub previous: Vec<Label>,
    #[serde(default)]
    pub current: Vec<Label>,
}

// ---- Pipeline (`object_kind: "pipeline"`) ----

#[derive(Debug, Deserialize)]
pub struct PipelineEvent {
    pub project: Project,
    pub user: User,
    pub object_attributes: PipelineAttrs,
    #[serde(default)]
    pub commit: Option<Commit>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineAttrs {
    pub status: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub url: String,
}

// ---- Note / comment (`object_kind: "note"`) ----

#[derive(Debug, Deserialize)]
pub struct NoteEvent {
    pub project: Project,
    pub user: User,
    pub object_attributes: NoteAttrs,
    #[serde(default)]
    pub merge_request: Option<NoteParent>,
    #[serde(default)]
    pub issue: Option<NoteParent>,
    #[serde(default)]
    pub snippet: Option<NoteParent>,
}

#[derive(Debug, Deserialize)]
pub struct NoteAttrs {
    pub note: String,
    pub noteable_type: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct NoteParent {
    pub title: String,
}

// ---- Push (`object_kind: "push"`) ----

#[derive(Debug, Deserialize)]
pub struct PushEvent {
    pub project: Project,
    pub user_username: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub total_commits_count: u64,
    #[serde(default)]
    pub commits: Vec<Commit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_merge_request_open() {
        let body = br#"{
            "object_kind": "merge_request",
            "user": {"username": "alice", "name": "Alice"},
            "project": {"path_with_namespace": "grp/demo", "web_url": "https://gitlab.com/grp/demo"},
            "object_attributes": {
                "iid": 7, "title": "Add feature", "state": "opened", "action": "open",
                "source_branch": "feat", "target_branch": "main",
                "url": "https://gitlab.com/grp/demo/-/merge_requests/7"
            },
            "reviewers": [{"username": "bob", "name": "Bob"}],
            "assignees": []
        }"#;
        let ev: MergeRequestEvent = serde_json::from_slice(body).unwrap();
        assert_eq!(ev.object_attributes.iid, 7);
        assert_eq!(ev.object_attributes.action.as_deref(), Some("open"));
        assert_eq!(ev.project.path_with_namespace, "grp/demo");
        assert_eq!(ev.reviewers.len(), 1);
        assert_eq!(ev.reviewers[0].username, "bob");
        assert!(ev.assignees.is_empty());
    }

    #[test]
    fn parses_issue_label_changes() {
        let body = br#"{
            "object_kind": "issue",
            "user": {"username": "alice", "name": "Alice"},
            "project": {"path_with_namespace": "grp/demo", "web_url": "https://gitlab.com/grp/demo"},
            "object_attributes": {"iid": 3, "title": "Broken", "url": "https://gitlab.com/grp/demo/-/issues/3"},
            "changes": {"labels": {"previous": [], "current": [{"title": "Bug"}]}}
        }"#;
        let ev: IssueEvent = serde_json::from_slice(body).unwrap();
        let labels = ev.changes.unwrap().labels.unwrap();
        assert!(labels.previous.is_empty());
        assert_eq!(labels.current[0].title, "Bug");
    }

    #[test]
    fn parses_pipeline_failed() {
        let body = br#"{
            "object_kind": "pipeline",
            "user": {"username": "alice", "name": "Alice"},
            "project": {"path_with_namespace": "grp/demo", "web_url": "https://gitlab.com/grp/demo"},
            "object_attributes": {"status": "failed", "ref": "main", "url": "https://gitlab.com/grp/demo/-/pipelines/9"},
            "commit": {"title": "Bad commit", "url": "https://gitlab.com/grp/demo/-/commit/abc"}
        }"#;
        let ev: PipelineEvent = serde_json::from_slice(body).unwrap();
        assert_eq!(ev.object_attributes.status, "failed");
        assert_eq!(ev.object_attributes.ref_name, "main");
        assert_eq!(ev.commit.unwrap().title, "Bad commit");
    }

    #[test]
    fn parses_note_on_merge_request() {
        let body = br#"{
            "object_kind": "note",
            "user": {"username": "alice", "name": "Alice"},
            "project": {"path_with_namespace": "grp/demo", "web_url": "https://gitlab.com/grp/demo"},
            "object_attributes": {"note": "looks good", "noteable_type": "MergeRequest", "url": "https://gitlab.com/grp/demo/-/merge_requests/7#note_1"},
            "merge_request": {"title": "Add feature"}
        }"#;
        let ev: NoteEvent = serde_json::from_slice(body).unwrap();
        assert_eq!(ev.object_attributes.noteable_type, "MergeRequest");
        assert_eq!(ev.merge_request.unwrap().title, "Add feature");
        assert!(ev.issue.is_none());
    }

    #[test]
    fn parses_push() {
        let body = br#"{
            "object_kind": "push",
            "user_username": "alice",
            "ref": "refs/heads/main",
            "total_commits_count": 2,
            "project": {"path_with_namespace": "grp/demo", "web_url": "https://gitlab.com/grp/demo"},
            "commits": [{"title": "First", "url": "https://gitlab.com/c/1"}]
        }"#;
        let ev: PushEvent = serde_json::from_slice(body).unwrap();
        assert_eq!(ev.ref_name, "refs/heads/main");
        assert_eq!(ev.total_commits_count, 2);
        assert_eq!(ev.commits.len(), 1);
    }
}
