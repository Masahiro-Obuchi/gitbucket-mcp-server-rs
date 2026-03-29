use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::issue::{CreateIssue, UpdateIssue};
use crate::server::GitBucketMcpServer;
use crate::tools::validation::{
    error, issue_state, list_state, optional_trimmed, required_trimmed,
};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIssuesParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Filter by state: open, closed, or all (default: open)")]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue title")]
    pub title: String,
    #[schemars(description = "Issue body/description")]
    pub body: Option<String>,
    #[schemars(description = "Label names to assign")]
    pub labels: Option<Vec<String>>,
    #[schemars(description = "Usernames to assign")]
    pub assignees: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
    #[schemars(description = "New state: open or closed")]
    pub state: Option<String>,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New body")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIssueCommentsParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddIssueCommentParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
    #[schemars(description = "Comment body text")]
    pub body: String,
}

#[tool_router(router = tool_router_issue, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List issues in a GitBucket repository")]
    pub async fn list_issues(&self, Parameters(params): Parameters<ListIssuesParams>) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };
        let state = match list_state(params.state) {
            Ok(state) => state,
            Err(err) => return err,
        };

        match self
            .client
            .list_issues(&owner, &repo, state.as_deref())
            .await
        {
            Ok(issues) => serde_json::to_string_pretty(&issues)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Get details of a specific issue in a GitBucket repository")]
    pub async fn get_issue(&self, Parameters(params): Parameters<GetIssueParams>) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };

        match self
            .client
            .get_issue(&owner, &repo, params.issue_number)
            .await
        {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Create a new issue in a GitBucket repository")]
    pub async fn create_issue(&self, Parameters(params): Parameters<CreateIssueParams>) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };
        let title = match required_trimmed(&params.title, "title") {
            Ok(title) => title,
            Err(err) => return err,
        };

        let body = CreateIssue {
            title,
            body: optional_trimmed(params.body),
            labels: params.labels,
            assignees: params.assignees,
        };
        match self.client.create_issue(&owner, &repo, &body).await {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(
        description = "Update an issue in a GitBucket repository (change state, title, or body)"
    )]
    pub async fn update_issue(&self, Parameters(params): Parameters<UpdateIssueParams>) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };
        let state = match issue_state(params.state) {
            Ok(state) => state,
            Err(err) => return err,
        };
        let title = match params.title {
            Some(title) => match required_trimmed(&title, "title") {
                Ok(title) => Some(title),
                Err(err) => return err,
            },
            None => None,
        };
        let body = optional_trimmed(params.body);

        if state.is_none() && title.is_none() && body.is_none() {
            return error("at least one of state, title, or body must be provided");
        }

        let body = UpdateIssue { state, title, body };
        match self
            .client
            .update_issue(&owner, &repo, params.issue_number, &body)
            .await
        {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "List comments on an issue in a GitBucket repository")]
    pub async fn list_issue_comments(
        &self,
        Parameters(params): Parameters<ListIssueCommentsParams>,
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };

        match self
            .client
            .list_issue_comments(&owner, &repo, params.issue_number)
            .await
        {
            Ok(comments) => serde_json::to_string_pretty(&comments)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Add a comment to an issue in a GitBucket repository")]
    pub async fn add_issue_comment(
        &self,
        Parameters(params): Parameters<AddIssueCommentParams>,
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };
        let comment = match required_trimmed(&params.body, "body") {
            Ok(comment) => comment,
            Err(err) => return err,
        };

        let body = CreateComment { body: comment };
        match self
            .client
            .add_issue_comment(&owner, &repo, params.issue_number, &body)
            .await
        {
            Ok(comment) => serde_json::to_string_pretty(&comment)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{MockApi, RecordedCall};

    #[tokio::test]
    async fn test_list_issues_rejects_invalid_state() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_issues(Parameters(ListIssuesParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                state: Some("draft".to_string()),
            }))
            .await;

        assert_eq!(result, "Error: state must be one of: open, closed, all");
    }

    #[tokio::test]
    async fn test_update_issue_requires_a_change() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .update_issue(Parameters(UpdateIssueParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                issue_number: 1,
                state: None,
                title: None,
                body: None,
            }))
            .await;

        assert_eq!(
            result,
            "Error: at least one of state, title, or body must be provided"
        );
    }

    #[tokio::test]
    async fn test_add_issue_comment_rejects_blank_body() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .add_issue_comment(Parameters(AddIssueCommentParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                issue_number: 1,
                body: "  ".to_string(),
            }))
            .await;

        assert_eq!(result, "Error: body must not be empty");
    }

    #[tokio::test]
    async fn test_update_issue_passes_trimmed_body_to_api_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .update_issue(Parameters(UpdateIssueParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                issue_number: 42,
                state: Some("closed".to_string()),
                title: Some("  New title  ".to_string()),
                body: Some("  Updated body  ".to_string()),
            }))
            .await;

        assert!(result.contains("\"title\": \"Mock issue\""));
        match mock.calls().as_slice() {
            [RecordedCall::UpdateIssue {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 42);
                assert_eq!(body.state.as_deref(), Some("closed"));
                assert_eq!(body.title.as_deref(), Some("New title"));
                assert_eq!(body.body.as_deref(), Some("Updated body"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
