use crate::models::list_page::{ListOptions, ListView};
use crate::tools::list_page::{ListKind, ListObservation, ListScope};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::pull_request::{
    CreatePullRequest, MergeHeadGuard, MergeMethod, MergePullRequest, UpdatePullRequest,
};
use crate::server::GitBucketMcpServer;
use crate::tools::response::{
    from_gb_error, read_only_error, success, validation_error, ToolResult,
};
use crate::tools::validation::{
    error, issue_state, list_state, repository_fields, required_optional_trimmed, required_text,
    required_trimmed,
};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPullRequestsParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Filter by state: open, closed, or all (default: open)")]
    pub state: Option<String>,
    #[schemars(
        description = "Maximum returned items: 1..100, default 50; byte limit may return fewer"
    )]
    pub limit: Option<u64>,
    #[schemars(
        description = "Server-issued continuation; reuse identical filters, view and limit before expiry"
    )]
    pub cursor: Option<String>,
    #[schemars(
        description = "summary (default) omits/previews fields explicitly; full returns complete model fields within byte limits"
    )]
    pub view: Option<ListView>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct UpdatePullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
    #[schemars(description = "New state: open or closed")]
    pub state: Option<String>,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New body")]
    pub body: Option<String>,
    #[schemars(description = "New base branch")]
    pub base: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MergePullRequestParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Pull request number")]
    pub pull_number: u64,
    #[schemars(
        description = "Optional merge/squash message text, sent unchanged with a native prefix; no full REST readback. Rebase rejects this field."
    )]
    pub commit_message: Option<String>,
    #[schemars(
        description = "merge (default), squash or rebase; merge maps to GitBucket merge-commit, repository policy remains enforced by the server"
    )]
    pub merge_method: Option<MergeMethod>,
    #[schemars(
        description = "Full lowercase SHA-1 of the reviewed head; requires explicit head_guard=preflight"
    )]
    pub expected_head_sha: Option<String>,
    #[schemars(
        description = "none (default) is ordinary merge; preflight requires expected_head_sha and accepts a race window; atomic is unsupported before HTTP"
    )]
    pub head_guard: Option<MergeHeadGuard>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
    #[tool(
        description = "List pull requests in a GitBucket repository. Paged response (default 50, max 100) with summary view, has_more/next_cursor/range; continue the same observation before expiry",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn list_pull_requests(
        &self,
        Parameters(params): Parameters<ListPullRequestsParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(err) => return validation_error(err),
        };
        let state = match list_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };

        self.list_response(
            ListScope::new(ListKind::PullRequests, &owner, Some(&repo)).state(state.as_deref()),
            ListOptions {
                limit: params.limit,
                cursor: params.cursor,
                view: params.view,
            },
            async {
                ListObservation::new(
                    self.client
                        .list_pull_requests(&owner, &repo, state.as_deref())
                        .await?,
                    None,
                )
            },
        )
        .await
    }

    #[tool(
        description = "Get details of a specific pull request in a GitBucket repository",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn get_pull_request(
        &self,
        Parameters(params): Parameters<GetPullRequestParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
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

    #[tool(
        description = "Create a new pull request in a GitBucket repository",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn create_pull_request(
        &self,
        Parameters(params): Parameters<CreatePullRequestParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
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
            body: params.body,
        };
        match self.client.create_pull_request(&owner, &repo, &body).await {
            Ok(pr) => success(&pr),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(
        description = "Update a pull request in a GitBucket repository (change state, title, body, or base branch)",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn update_pull_request(
        &self,
        Parameters(params): Parameters<UpdatePullRequestParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(err) => return validation_error(err),
        };
        let state = match issue_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };
        let title = match required_optional_trimmed(params.title, "title") {
            Ok(title) => title,
            Err(err) => return validation_error(err),
        };
        let body = params.body;
        let base = match required_optional_trimmed(params.base, "base") {
            Ok(base) => base,
            Err(err) => return validation_error(err),
        };

        if state.is_none() && title.is_none() && body.is_none() && base.is_none() {
            return validation_error(error(
                "at least one of state, title, body, or base must be provided",
            ));
        }

        let body = UpdatePullRequest {
            state,
            title,
            body,
            base,
        };
        match self
            .client
            .update_pull_request(&owner, &repo, params.pull_number, &body)
            .await
        {
            Ok(pr) => success(&pr),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(
        description = "Merge once using merge/squash/rebase, then verify PR state, result commit parents and base tip. Optional expected_head_sha requires explicit head_guard=preflight with a race window; atomic is unsupported before HTTP. Unknown mergeability remains unknown; draft/unknown draft is rejected. No retry or strategy fallback.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn merge_pull_request(
        &self,
        Parameters(params): Parameters<MergePullRequestParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(err) => return validation_error(err),
        };

        let body = MergePullRequest {
            commit_message: params.commit_message,
            expected_head_sha: params.expected_head_sha,
            merge_method: params.merge_method.unwrap_or_default(),
            head_guard: params.head_guard.unwrap_or_default(),
        };
        if params.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        if let Err(error) = body.validate() {
            return from_gb_error(error);
        }
        match self
            .client
            .merge_pull_request(&owner, &repo, params.pull_number, &body)
            .await
        {
            Ok(result) => success(&result),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(
        description = "Add a comment to a pull request in a GitBucket repository",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn add_pull_request_comment(
        &self,
        Parameters(params): Parameters<AddPullRequestCommentParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(err) => return validation_error(err),
        };
        let comment = match required_text(&params.body, "body") {
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

    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{error_payload, success_json, MockApi, RecordedCall};
    use crate::tools::response::ToolErrorPayload;

    #[tokio::test]
    async fn test_list_pull_requests_rejects_invalid_state() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_pull_requests(Parameters(ListPullRequestsParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                state: Some("merged".to_string()),
                limit: None,
                cursor: None,
                view: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "state must be one of: open, closed, all".to_string(),
                status: None,
                operation: None,
                cause: None,
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
                operation: None,
                cause: None,
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
                operation: None,
                cause: None,
            }
        );
    }

    #[tokio::test]
    async fn test_update_pull_request_rejects_empty_change_set() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .update_pull_request(Parameters(UpdatePullRequestParams {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
                pull_number: 7,
                state: None,
                title: None,
                body: None,
                base: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "at least one of state, title, body, or base must be provided".to_string(),
                status: None,
                operation: None,
                cause: None,
            }
        );
    }

    #[tokio::test]
    async fn test_create_pull_request_preserves_body_and_normalizes_identifiers() {
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
                assert_eq!(body.body.as_deref(), Some("  PR body  "));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_update_pull_request_passes_trimmed_fields_to_api_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .update_pull_request(Parameters(UpdatePullRequestParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                pull_number: 7,
                state: Some("closed".to_string()),
                title: Some("  Updated PR  ".to_string()),
                body: Some("  Updated body  ".to_string()),
                base: Some("  develop  ".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["title"].as_str(), Some("Mock PR"));
        match mock.calls().as_slice() {
            [RecordedCall::UpdatePullRequest {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
                assert_eq!(body.state.as_deref(), Some("closed"));
                assert_eq!(body.title.as_deref(), Some("Updated PR"));
                assert_eq!(body.body.as_deref(), Some("  Updated body  "));
                assert_eq!(body.base.as_deref(), Some("develop"));
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
                limit: None,
                cursor: None,
                view: None,
            }))
            .await;

        let result = success_json(result);
        assert_eq!(
            result["pull_requests"][0]["title"].as_str(),
            Some("Mock PR")
        );
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
    async fn test_merge_pull_request_preserves_message_and_normalizes_identifiers() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .merge_pull_request(Parameters(MergePullRequestParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                pull_number: 7,
                commit_message: Some("  merge message  ".to_string()),
                merge_method: None,
                expected_head_sha: None,
                head_guard: None,
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
                assert_eq!(body.commit_message.as_deref(), Some("  merge message  "));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_add_pull_request_comment_preserves_body_and_normalizes_identifiers() {
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
                assert_eq!(body.body, "  Looks good  ");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
