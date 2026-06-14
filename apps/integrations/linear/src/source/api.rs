//! Linear GraphQL API client. Backs link previews, the due-date reminder
//! scheduler ([`fetch_issues_due_soon`](LinearClient::fetch_issues_due_soon)),
//! and webhook subscriber fan-out
//! ([`fetch_issue_subscribers`](LinearClient::fetch_issue_subscribers)).

use graphql_client::{GraphQLQuery, Response};
use reqwest::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

// graphql_client maps these custom scalars to the Rust types in scope here.
// Linear's `TimelessDate` is a `YYYY-MM-DD` string; `TimelessDateOrDuration`
// accepts the same (or an ISO-8601 duration) — we only ever send dates.
type TimelessDate = String;
type TimelessDateOrDuration = String;

/// A single issue lookup by team key + issue number, type-checked at build time
/// against `graphql/schema.graphql`.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/issue_by_number.graphql",
    response_derives = "Debug"
)]
struct IssueByNumber;

/// Active, due-dated issues within a date window — the reminder scheduler's feed.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/issues_due_soon.graphql",
    response_derives = "Debug"
)]
struct IssuesDueSoon;

/// An issue's current subscribers — used to fan webhook activity out to people.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/issue_subscribers.graphql",
    response_derives = "Debug"
)]
struct IssueSubscribers;

/// Client for the Linear GraphQL API.
pub struct LinearClient {
    api_key: String,
    http: Client,
}

impl LinearClient {
    pub fn new(api_key: String, http: Client) -> Self {
        Self { api_key, http }
    }

    /// Execute a typed GraphQL operation and return its `data`, surfacing
    /// transport, parse, and GraphQL-level errors as `Err(String)`.
    async fn run<Q>(&self, variables: Q::Variables) -> Result<Q::ResponseData, String>
    where
        Q: GraphQLQuery,
        Q::Variables: Serialize,
        Q::ResponseData: DeserializeOwned,
    {
        let resp = self
            .http
            .post(LINEAR_API_URL)
            .header("Authorization", &self.api_key)
            .json(&Q::build_query(variables))
            .send()
            .await
            .map_err(|e| format!("Linear API request failed: {e}"))?;

        let body: Response<Q::ResponseData> = resp
            .json()
            .await
            .map_err(|e| format!("Linear API response parse failed: {e}"))?;

        if let Some(errors) = body.errors.filter(|e| !e.is_empty()) {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("Linear GraphQL errors: {}", messages.join("; ")));
        }

        body.data
            .ok_or_else(|| "Linear API returned no data".to_string())
    }

    /// Fetches a single issue by its identifier (e.g. `"ABX-16"`).
    pub async fn fetch_issue_by_identifier(
        &self,
        identifier: &str,
    ) -> Result<LinearIssueData, String> {
        let (team_key, number_str) = identifier
            .rsplit_once('-')
            .ok_or_else(|| format!("invalid identifier format: {identifier}"))?;
        let number: u32 = number_str
            .parse()
            .map_err(|_| format!("invalid issue number in identifier: {identifier}"))?;

        let data = self
            .run::<IssueByNumber>(issue_by_number::Variables {
                team_key: team_key.to_string(),
                number: number.into(),
            })
            .await?;

        let node = data
            .issues
            .nodes
            .into_iter()
            .next()
            .ok_or_else(|| format!("no issue found for identifier '{identifier}'"))?;

        Ok(LinearIssueData {
            title: node.title,
            description: node.description,
            priority: node.priority as u8,
            state: LinearIssueState {
                name: node.state.name,
            },
            assignee: node.assignee.map(|a| LinearIssueAssignee { name: a.name }),
            url: node.url,
            identifier: node.identifier,
        })
    }

    /// Fetches active, assigned, due-dated issues whose `dueDate` falls within
    /// `[gte, lte]` (both `YYYY-MM-DD`). Paginates to completion.
    pub async fn fetch_issues_due_soon(
        &self,
        gte: &str,
        lte: &str,
    ) -> Result<Vec<DueIssue>, String> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let data = self
                .run::<IssuesDueSoon>(issues_due_soon::Variables {
                    after: after.clone(),
                    gte: gte.to_string(),
                    lte: lte.to_string(),
                })
                .await?;
            let conn = data.issues;
            for n in conn.nodes {
                // Filtered to non-null due dates server-side, but the field is
                // nullable — skip defensively rather than unwrap.
                let Some(due_date) = n.due_date else { continue };
                out.push(DueIssue {
                    id: n.id,
                    identifier: n.identifier,
                    title: n.title,
                    url: n.url,
                    due_date,
                    priority: n.priority as u8,
                    state: n.state.name,
                    assignee: n.assignee.map(|a| DueAssignee {
                        name: a.name,
                        email: a.email,
                    }),
                    subscribers: n
                        .subscribers
                        .nodes
                        .into_iter()
                        .map(|s| LinearUser {
                            id: s.id,
                            name: s.name,
                            email: s.email,
                            active: s.active,
                        })
                        .collect(),
                });
            }
            match conn.page_info.has_next_page {
                true => {
                    after = conn.page_info.end_cursor;
                    if after.is_none() {
                        break;
                    }
                }
                false => break,
            }
        }
        Ok(out)
    }

    /// Fetches an issue's current subscribers (for webhook fan-out).
    pub async fn fetch_issue_subscribers(&self, id: &str) -> Result<IssueSubscriberInfo, String> {
        let data = self
            .run::<IssueSubscribers>(issue_subscribers::Variables { id: id.to_string() })
            .await?;
        let issue = data.issue;
        Ok(IssueSubscriberInfo {
            identifier: issue.identifier,
            title: issue.title,
            url: issue.url,
            priority: issue.priority as u8,
            state: issue.state.name,
            subscribers: issue
                .subscribers
                .nodes
                .into_iter()
                .map(|s| LinearUser {
                    id: s.id,
                    name: s.name,
                    email: s.email,
                    active: s.active,
                })
                .collect(),
        })
    }
}

/// Extracts an issue identifier (e.g. `"LIN-123"`) from a Linear URL like
/// `https://linear.app/workspace/issue/LIN-123/some-slug`.
pub fn extract_identifier_from_url(url: &str) -> Option<String> {
    let parts: Vec<&str> = url.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if *part == "issue"
            && let Some(ident) = parts.get(i + 1)
            && ident.contains('-')
            && ident
                .split('-')
                .next_back()
                .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(ident.to_string());
        }
    }
    None
}

/// Issue data for link previews, mapped from the typed `IssueByNumber`
/// GraphQL response.
#[derive(Debug)]
pub struct LinearIssueData {
    pub title: String,
    pub description: Option<String>,
    pub priority: u8,
    pub state: LinearIssueState,
    pub assignee: Option<LinearIssueAssignee>,
    pub url: String,
    pub identifier: String,
}

#[derive(Debug)]
pub struct LinearIssueState {
    pub name: String,
}

#[derive(Debug)]
pub struct LinearIssueAssignee {
    pub name: String,
}

/// A user attached to an issue (subscriber, or reminder fan-out target).
#[derive(Debug, Clone)]
pub struct LinearUser {
    pub id: String,
    pub name: String,
    pub email: String,
    pub active: bool,
}

/// An active, due-dated issue from [`LinearClient::fetch_issues_due_soon`].
#[derive(Debug)]
pub struct DueIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    /// `YYYY-MM-DD`.
    pub due_date: String,
    pub priority: u8,
    pub state: String,
    pub assignee: Option<DueAssignee>,
    pub subscribers: Vec<LinearUser>,
}

#[derive(Debug)]
pub struct DueAssignee {
    pub name: String,
    pub email: String,
}

/// An issue's subscriber set from [`LinearClient::fetch_issue_subscribers`].
#[derive(Debug)]
pub struct IssueSubscriberInfo {
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub priority: u8,
    pub state: String,
    pub subscribers: Vec<LinearUser>,
}
