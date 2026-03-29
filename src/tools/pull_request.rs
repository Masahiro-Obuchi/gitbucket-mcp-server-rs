use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::pull_request::{CreatePullRequest, MergePullRequest};
use crate::server::GitBucketMcpServer;

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
    ) -> String {
        match self
            .client
            .list_pull_requests(&params.owner, &params.repo, params.state.as_deref())
            .await
        {
            Ok(prs) => serde_json::to_string_pretty(&prs)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Get details of a specific pull request in a GitBucket repository")]
    pub async fn get_pull_request(
        &self,
        Parameters(params): Parameters<GetPullRequestParams>,
    ) -> String {
        match self
            .client
            .get_pull_request(&params.owner, &params.repo, params.pull_number)
            .await
        {
            Ok(pr) => serde_json::to_string_pretty(&pr)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Create a new pull request in a GitBucket repository")]
    pub async fn create_pull_request(
        &self,
        Parameters(params): Parameters<CreatePullRequestParams>,
    ) -> String {
        let body = CreatePullRequest {
            title: params.title,
            head: params.head,
            base: params.base,
            body: params.body,
        };
        match self
            .client
            .create_pull_request(&params.owner, &params.repo, &body)
            .await
        {
            Ok(pr) => serde_json::to_string_pretty(&pr)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Merge a pull request in a GitBucket repository")]
    pub async fn merge_pull_request(
        &self,
        Parameters(params): Parameters<MergePullRequestParams>,
    ) -> String {
        let body = MergePullRequest {
            commit_message: params.commit_message,
            sha: None,
            merge_method: None,
        };
        match self
            .client
            .merge_pull_request(&params.owner, &params.repo, params.pull_number, &body)
            .await
        {
            Ok(result) => serde_json::to_string_pretty(&result)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Add a comment to a pull request in a GitBucket repository")]
    pub async fn add_pull_request_comment(
        &self,
        Parameters(params): Parameters<AddPullRequestCommentParams>,
    ) -> String {
        let body = CreateComment { body: params.body };
        match self
            .client
            .add_pull_request_comment(&params.owner, &params.repo, params.pull_number, &body)
            .await
        {
            Ok(comment) => serde_json::to_string_pretty(&comment)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}
