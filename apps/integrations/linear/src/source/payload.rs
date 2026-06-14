//! Deserialization types for Linear webhook payloads and GraphQL responses.

use serde::Deserialize;

/// Top-level Linear webhook payload.
#[derive(Debug, Deserialize)]
pub struct LinearPayload {
    pub action: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: serde_json::Value,
    pub url: String,
    #[serde(rename = "updatedFrom")]
    pub updated_from: Option<serde_json::Value>,
    /// The user who triggered the event (when present). Kept untyped so an
    /// unexpected shape never fails payload parsing; the id is read via
    /// [`LinearPayload::actor_id`] to exclude self from subscriber fan-out.
    #[serde(default)]
    pub actor: Option<serde_json::Value>,
}

impl LinearPayload {
    /// The triggering user's id, if the payload carries an `actor`.
    pub fn actor_id(&self) -> Option<String> {
        self.actor
            .as_ref()
            .and_then(|a| a.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}

/// Issue data embedded in a webhook payload.
#[derive(Debug, Deserialize)]
pub struct Issue {
    pub id: String,
    pub title: String,
    pub priority: u8,
    pub state: IssueState,
    pub assignee: Option<Assignee>,
    pub identifier: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IssueState {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Assignee {
    pub name: String,
    pub email: Option<String>,
}

/// Previous field values sent in an `"update"` webhook for change detection.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedFrom {
    #[serde(default)]
    pub state: Option<serde_json::Value>,
    #[serde(default)]
    pub priority: Option<u8>,
    #[serde(default)]
    pub assignee: Option<serde_json::Value>,
    #[serde(default)]
    pub assignee_id: Option<String>,
}

/// Comment data embedded in a webhook payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentData {
    #[allow(dead_code)]
    pub id: String,
    pub body: String,
    /// The commented issue's id (Linear sends `issueId` alongside `issue`); used
    /// to fetch subscribers for fan-out.
    #[serde(default)]
    pub issue_id: Option<String>,
    pub issue: Option<CommentIssue>,
}

#[derive(Debug, Deserialize)]
pub struct CommentIssue {
    pub identifier: String,
    pub title: String,
}

/// Actor (user) attached to a webhook event.
#[derive(Debug, Deserialize)]
pub struct Actor {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[allow(dead_code)]
    pub email: Option<String>,
}
