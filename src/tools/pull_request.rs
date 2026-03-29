use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::pull_request::{CreatePullRequest, MergePullRequest};
use crate::server::GitBucketMcpServer;
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
    ) -> String {
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
            .list_pull_requests(&owner, &repo, state.as_deref())
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
            .get_pull_request(&owner, &repo, params.pull_number)
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
        let head = match required_trimmed(&params.head, "head") {
            Ok(head) => head,
            Err(err) => return err,
        };
        let base = match required_trimmed(&params.base, "base") {
            Ok(base) => base,
            Err(err) => return err,
        };

        let body = CreatePullRequest {
            title,
            head,
            base,
            body: optional_trimmed(params.body),
        };
        match self.client.create_pull_request(&owner, &repo, &body).await {
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
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
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
            .add_pull_request_comment(&owner, &repo, params.pull_number, &body)
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
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    use crate::api::client::GitBucketClient;

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

        assert_eq!(result, "Error: state must be one of: open, closed, all");
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

        assert_eq!(result, "Error: head must not be empty");
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

        assert_eq!(result, "Error: body must not be empty");
    }
}
