use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::repository::CreateRepository;
use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, success_list, validation_error, ToolResult};
use crate::tools::validation::required_trimmed;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRepositoriesParams {
    #[schemars(description = "Repository owner (username or organization name)")]
    pub owner: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetRepositoryParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRepositoryParams {
    #[schemars(description = "Name of the new repository")]
    pub name: String,
    #[schemars(description = "Description of the repository")]
    pub description: Option<String>,
    #[schemars(description = "Whether the repository should be private (default: false)")]
    pub private: Option<bool>,
    #[schemars(description = "Initialize with a README (default: false)")]
    pub auto_init: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForkRepositoryParams {
    #[schemars(description = "Owner of the repository to fork")]
    pub owner: String,
    #[schemars(description = "Name of the repository to fork")]
    pub repo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBranchesParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
}

#[tool_router(router = tool_router_repository, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List repositories for a user or organization in GitBucket")]
    pub async fn list_repositories(
        &self,
        Parameters(params): Parameters<ListRepositoriesParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };

        match self.client.list_repositories(&owner).await {
            Ok(repos) => success_list("repositories", &repos),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Get details of a GitBucket repository")]
    pub async fn get_repository(
        &self,
        Parameters(params): Parameters<GetRepositoryParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self.client.get_repository(&owner, &repo).await {
            Ok(repo) => success(&repo),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Create a new repository in GitBucket for the authenticated user")]
    pub async fn create_repository(
        &self,
        Parameters(params): Parameters<CreateRepositoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return validation_error(err),
        };

        let body = CreateRepository {
            name,
            description: params.description,
            is_private: params.private,
            auto_init: params.auto_init,
        };
        match self.client.create_repository(&body).await {
            Ok(repo) => success(&repo),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Fork a GitBucket repository")]
    pub async fn fork_repository(
        &self,
        Parameters(params): Parameters<ForkRepositoryParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self.client.fork_repository(&owner, &repo).await {
            Ok(repo) => success(&repo),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "List branches for a GitBucket repository")]
    pub async fn list_branches(
        &self,
        Parameters(params): Parameters<ListBranchesParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self.client.list_branches(&owner, &repo).await {
            Ok(branches) => success_list("branches", &branches),
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
    async fn test_list_repositories_rejects_blank_owner() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_repositories(Parameters(ListRepositoriesParams {
                owner: "   ".to_string(),
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "owner must not be empty".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_create_repository_rejects_blank_name() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .create_repository(Parameters(CreateRepositoryParams {
                name: "   ".to_string(),
                description: None,
                private: None,
                auto_init: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "name must not be empty".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_create_repository_passes_body_to_api_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .create_repository(Parameters(CreateRepositoryParams {
                name: "  new-repo  ".to_string(),
                description: Some("A repository".to_string()),
                private: Some(true),
                auto_init: Some(true),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["full_name"].as_str(), Some("mock-user/mock-repo"));
        match mock.calls().as_slice() {
            [RecordedCall::CreateRepository { body }] => {
                assert_eq!(body.name, "new-repo");
                assert_eq!(body.description.as_deref(), Some("A repository"));
                assert_eq!(body.is_private, Some(true));
                assert_eq!(body.auto_init, Some(true));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_repositories_passes_trimmed_owner_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_repositories(Parameters(ListRepositoriesParams {
                owner: "  mock-user  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(
            result["repositories"][0]["full_name"].as_str(),
            Some("mock-user/mock-repo")
        );
        match mock.calls().as_slice() {
            [RecordedCall::ListRepositories { owner }] => assert_eq!(owner, "mock-user"),
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_repository_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_repository(Parameters(GetRepositoryParams {
                owner: "  mock-user ".to_string(),
                repo: " mock-repo  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["name"].as_str(), Some("mock-repo"));
        match mock.calls().as_slice() {
            [RecordedCall::GetRepository { owner, repo }] => {
                assert_eq!(owner, "mock-user");
                assert_eq!(repo, "mock-repo");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_fork_repository_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .fork_repository(Parameters(ForkRepositoryParams {
                owner: "  upstream ".to_string(),
                repo: " sample-repo  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["full_name"].as_str(), Some("mock-user/mock-repo"));
        match mock.calls().as_slice() {
            [RecordedCall::ForkRepository { owner, repo }] => {
                assert_eq!(owner, "upstream");
                assert_eq!(repo, "sample-repo");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_branches_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_branches(Parameters(ListBranchesParams {
                owner: "  mock-user ".to_string(),
                repo: " mock-repo ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["branches"][0]["name"].as_str(), Some("main"));
        match mock.calls().as_slice() {
            [RecordedCall::ListBranches { owner, repo }] => {
                assert_eq!(owner, "mock-user");
                assert_eq!(repo, "mock-repo");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
