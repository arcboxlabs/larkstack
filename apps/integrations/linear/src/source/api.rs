//! Linear GraphQL API client for fetching issue data (used by link previews).

use graphql_client::{GraphQLQuery, Response};
use reqwest::Client;

const LINEAR_API_URL: &str = "https://api.linear.app/graphql";

/// A single issue lookup by team key + issue number, type-checked at build time
/// against `graphql/schema.graphql`.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/issue_by_number.graphql",
    response_derives = "Debug"
)]
struct IssueByNumber;

/// Client for the Linear GraphQL API.
pub struct LinearClient {
    api_key: String,
    http: Client,
}

impl LinearClient {
    pub fn new(api_key: String, http: Client) -> Self {
        Self { api_key, http }
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

        let variables = issue_by_number::Variables {
            team_key: team_key.to_string(),
            number: number.into(),
        };

        let resp = self
            .http
            .post(LINEAR_API_URL)
            .header("Authorization", &self.api_key)
            .json(&IssueByNumber::build_query(variables))
            .send()
            .await
            .map_err(|e| format!("Linear API request failed: {e}"))?;

        let body: Response<issue_by_number::ResponseData> = resp
            .json()
            .await
            .map_err(|e| format!("Linear API response parse failed: {e}"))?;

        if let Some(errors) = body.errors.filter(|e| !e.is_empty()) {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("Linear GraphQL errors: {}", messages.join("; ")));
        }

        let node = body
            .data
            .and_then(|data| data.issues.nodes.into_iter().next())
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
