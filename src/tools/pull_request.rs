use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::pull_request::{CreatePullRequest, MergePullRequest};
use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, validation_error, ToolResult};
use crate::tools::validation::{list_state, optional_trimmed, required_trimmed};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPullRequestsParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Filter by state: open, closed, or all (default: open)")]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreatePullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request title")]
    pub title: String,
    #[schemars(description = "Branch containing changes (head branch)")]
    pub head: String,
    #[schemars(description = "Branch to merge into (base branch)")]
    pub base: String,
    #[schemars(description = "Pull request body/description")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MergePullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
    #[schemars(description = "Merge commit message")]
    pub commit_message: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddPullRequestCommentParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
    #[schemars(description = "Comment body text")]
    pub body: String,
}

#[tool_router(router = tool_router_pull_request, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List pull requests in a GitBucket repository")]
    pub async fn list_pull_requests(
        &self,
        Parameters(params): Parameters<ListPullRequestsParams>,
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
            .list_pull_requests(&owner, &repo, state.as_deref())
            .await
        {
            Ok(prs) => success(&prs),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Get details of a specific pull request in a GitBucket repository")]
    pub async fn get_pull_request(
        &self,
        Parameters(params): Parameters<GetPullRequestParams>,
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
            .get_pull_request(&owner, &repo, params.pull_number)
            .await
        {
            Ok(pr) => success(&pr),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Create a new pull request in a GitBucket repository")]
    pub async fn create_pull_request(
        &self,
        Parameters(params): Parameters<CreatePullRequestParams>,
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
        let head = match required_trimmed(&params.head, "head") {
            Ok(head) => head,
            Err(err) => return validation_error(err),
        };
        let base = match required_trimmed(&params.base, "base") {
            Ok(base) => base,
            Err(err) => return validation_error(err),
        };

        let body = CreatePullRequest {
            title,
            head,
            base,
            body: optional_trimmed(params.body),
        };
        match self.client.create_pull_request(&owner, &repo, &body).await {
            Ok(pr) => success(&pr),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Merge a pull request in a GitBucket repository")]
    pub async fn merge_pull_request(
        &self,
        Parameters(params): Parameters<MergePullRequestParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        let body = MergePullRequest {
            commit_message: optional_trimmed(params.commit_message),
            sha: None,
            merge_method: None,
        };
        match self
            .client
            .merge_pull_request(&owner, &repo, params.pull_number, &body)
            .await
        {
            Ok(result) => success(&result),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Add a comment to a pull request in a GitBucket repository")]
    pub async fn add_pull_request_comment(
        &self,
        Parameters(params): Parameters<AddPullRequestCommentParams>,
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
            .add_pull_request_comment(&owner, &repo, params.pull_number, &body)
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
    async fn test_list_pull_requests_rejects_invalid_state() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_pull_requests(Parameters(ListPullRequestsParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                state: Some("merged".to_string()),
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
    async fn test_create_pull_request_rejects_blank_head() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .create_pull_request(Parameters(CreatePullRequestParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                title: "Title".to_string(),
                head: "  ".to_string(),
                base: "main".to_string(),
                body: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "head must not be empty".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_add_pull_request_comment_rejects_blank_body() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .add_pull_request_comment(Parameters(AddPullRequestCommentParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                pull_number: 1,
                body: "   ".to_string(),
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
    async fn test_create_pull_request_passes_trimmed_fields_to_api_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .create_pull_request(Parameters(CreatePullRequestParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                title: "  Add feature  ".to_string(),
                head: "  feature-branch  ".to_string(),
                base: "  main  ".to_string(),
                body: Some("  PR body  ".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["title"].as_str(), Some("Mock PR"));
        match mock.calls().as_slice() {
            [RecordedCall::CreatePullRequest { owner, repo, body }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(body.title, "Add feature");
                assert_eq!(body.head, "feature-branch");
                assert_eq!(body.base, "main");
                assert_eq!(body.body.as_deref(), Some("PR body"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_pull_requests_passes_trimmed_fields_and_state_to_api() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_pull_requests(Parameters(ListPullRequestsParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                state: Some("closed".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result[0]["title"].as_str(), Some("Mock PR"));
        match mock.calls().as_slice() {
            [RecordedCall::ListPullRequests { owner, repo, state }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(state.as_deref(), Some("closed"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_pull_request_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_pull_request(Parameters(GetPullRequestParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                pull_number: 7,
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["number"].as_u64(), Some(7));
        match mock.calls().as_slice() {
            [RecordedCall::GetPullRequest {
                owner,
                repo,
                number,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_merge_pull_request_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .merge_pull_request(Parameters(MergePullRequestParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                pull_number: 7,
                commit_message: Some("  merge message  ".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["merged"].as_bool(), Some(true));
        match mock.calls().as_slice() {
            [RecordedCall::MergePullRequest {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
                assert_eq!(body.commit_message.as_deref(), Some("merge message"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_add_pull_request_comment_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .add_pull_request_comment(Parameters(AddPullRequestCommentParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                pull_number: 7,
                body: "  Looks good  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["body"].as_str(), Some("Mock comment"));
        match mock.calls().as_slice() {
            [RecordedCall::AddPullRequestComment {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
                assert_eq!(body.body, "Looks good");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
