use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::issue::{CreateIssue, UpdateIssue};
use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, validation_error, ToolResult};
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
    pub async fn list_issues(
        &self,
        Parameters(params): Parameters<ListIssuesParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let state = match list_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .list_issues(&owner, &repo, state.as_deref())
            .await
        {
            Ok(issues) => success(&issues),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Get details of a specific issue in a GitBucket repository")]
    pub async fn get_issue(&self, Parameters(params): Parameters<GetIssueParams>) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .get_issue(&owner, &repo, params.issue_number)
            .await
        {
            Ok(issue) => success(&issue),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Create a new issue in a GitBucket repository")]
    pub async fn create_issue(
        &self,
        Parameters(params): Parameters<CreateIssueParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let title = match required_trimmed(&params.title, "title") {
            Ok(title) => title,
            Err(err) => return validation_error(err),
        };

        let body = CreateIssue {
            title,
            body: optional_trimmed(params.body),
            labels: params.labels,
            assignees: params.assignees,
        };
        match self.client.create_issue(&owner, &repo, &body).await {
            Ok(issue) => success(&issue),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(
        description = "Update an issue in a GitBucket repository (change state, title, or body)"
    )]
    pub async fn update_issue(
        &self,
        Parameters(params): Parameters<UpdateIssueParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let state = match issue_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };
        let title = match params.title {
            Some(title) => match required_trimmed(&title, "title") {
                Ok(title) => Some(title),
                Err(err) => return validation_error(err),
            },
            None => None,
        };
        let body = optional_trimmed(params.body);

        if state.is_none() && title.is_none() && body.is_none() {
            return validation_error(error(
                "at least one of state, title, or body must be provided",
            ));
        }

        let body = UpdateIssue { state, title, body };
        match self
            .client
            .update_issue(&owner, &repo, params.issue_number, &body)
            .await
        {
            Ok(issue) => success(&issue),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "List comments on an issue in a GitBucket repository")]
    pub async fn list_issue_comments(
        &self,
        Parameters(params): Parameters<ListIssueCommentsParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .list_issue_comments(&owner, &repo, params.issue_number)
            .await
        {
            Ok(comments) => success(&comments),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Add a comment to an issue in a GitBucket repository")]
    pub async fn add_issue_comment(
        &self,
        Parameters(params): Parameters<AddIssueCommentParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let comment = match required_trimmed(&params.body, "body") {
            Ok(comment) => comment,
            Err(err) => return validation_error(err),
        };

        let body = CreateComment { body: comment };
        match self
            .client
            .add_issue_comment(&owner, &repo, params.issue_number, &body)
            .await
        {
            Ok(comment) => success(&comment),
            Err(e) => from_gb_error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;

    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{MockApi, RecordedCall};
    use crate::tools::response::ToolErrorPayload;

    fn success_json(result: ToolResult) -> Value {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(false));
        result
            .structured_content
            .expect("expected structured content for success")
    }

    fn error_payload(result: ToolResult) -> ToolErrorPayload {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        serde_json::from_value(
            result
                .structured_content
                .expect("expected structured content for error"),
        )
        .expect("error payload should deserialize")
    }

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

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "state must be one of: open, closed, all".to_string(),
                status: None,
            }
        );
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
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "at least one of state, title, or body must be provided".to_string(),
                status: None,
            }
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

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "body must not be empty".to_string(),
                status: None,
            }
        );
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

        let result = success_json(result);
        assert_eq!(result["title"].as_str(), Some("Mock issue"));
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

    #[tokio::test]
    async fn test_list_issues_passes_trimmed_fields_and_state_to_api() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_issues(Parameters(ListIssuesParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                state: Some("closed".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result[0]["title"].as_str(), Some("Mock issue"));
        match mock.calls().as_slice() {
            [RecordedCall::ListIssues { owner, repo, state }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(state.as_deref(), Some("closed"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_issue_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_issue(Parameters(GetIssueParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                issue_number: 42,
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["number"].as_u64(), Some(42));
        match mock.calls().as_slice() {
            [RecordedCall::GetIssue {
                owner,
                repo,
                number,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 42);
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_issue_passes_body_to_api_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .create_issue(Parameters(CreateIssueParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                title: "  New issue  ".to_string(),
                body: Some("  body text  ".to_string()),
                labels: Some(vec!["bug".to_string()]),
                assignees: Some(vec!["alice".to_string()]),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["title"].as_str(), Some("Mock issue"));
        match mock.calls().as_slice() {
            [RecordedCall::CreateIssue { owner, repo, body }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(body.title, "New issue");
                assert_eq!(body.body.as_deref(), Some("body text"));
                assert_eq!(body.labels.as_deref(), Some(&["bug".to_string()][..]));
                assert_eq!(body.assignees.as_deref(), Some(&["alice".to_string()][..]));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_issue_comments_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_issue_comments(Parameters(ListIssueCommentsParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                issue_number: 42,
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result[0]["body"].as_str(), Some("Mock comment"));
        match mock.calls().as_slice() {
            [RecordedCall::ListIssueComments {
                owner,
                repo,
                number,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 42);
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_add_issue_comment_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .add_issue_comment(Parameters(AddIssueCommentParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                issue_number: 42,
                body: "  Nice work  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["body"].as_str(), Some("Mock comment"));
        match mock.calls().as_slice() {
            [RecordedCall::AddIssueComment {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 42);
                assert_eq!(body.body, "Nice work");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
