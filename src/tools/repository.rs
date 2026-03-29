use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::repository::CreateRepository;
use crate::server::GitBucketMcpServer;
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
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };

        match self.client.list_repositories(&owner).await {
            Ok(repos) => serde_json::to_string_pretty(&repos)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Get details of a GitBucket repository")]
    pub async fn get_repository(
        &self,
        Parameters(params): Parameters<GetRepositoryParams>,
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };

        match self.client.get_repository(&owner, &repo).await {
            Ok(repo) => serde_json::to_string_pretty(&repo)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Create a new repository in GitBucket for the authenticated user")]
    pub async fn create_repository(
        &self,
        Parameters(params): Parameters<CreateRepositoryParams>,
    ) -> String {
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return err,
        };

        let body = CreateRepository {
            name,
            description: params.description,
            is_private: params.private,
            auto_init: params.auto_init,
        };
        match self.client.create_repository(&body).await {
            Ok(repo) => serde_json::to_string_pretty(&repo)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Fork a GitBucket repository")]
    pub async fn fork_repository(
        &self,
        Parameters(params): Parameters<ForkRepositoryParams>,
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };

        match self.client.fork_repository(&owner, &repo).await {
            Ok(repo) => serde_json::to_string_pretty(&repo)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "List branches for a GitBucket repository")]
    pub async fn list_branches(
        &self,
        Parameters(params): Parameters<ListBranchesParams>,
    ) -> String {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return err,
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return err,
        };

        match self.client.list_branches(&owner, &repo).await {
            Ok(branches) => serde_json::to_string_pretty(&branches)
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
    async fn test_list_repositories_rejects_blank_owner() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_repositories(Parameters(ListRepositoriesParams {
                owner: "   ".to_string(),
            }))
            .await;

        assert_eq!(result, "Error: owner must not be empty");
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

        assert_eq!(result, "Error: name must not be empty");
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

        assert!(result.contains("\"full_name\": \"mock-user/mock-repo\""));
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

        assert!(result.contains("\"full_name\": \"mock-user/mock-repo\""));
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

        assert!(result.contains("\"name\": \"mock-repo\""));
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

        assert!(result.contains("\"full_name\": \"mock-user/mock-repo\""));
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

        assert!(result.contains("\"name\": \"main\""));
        match mock.calls().as_slice() {
            [RecordedCall::ListBranches { owner, repo }] => {
                assert_eq!(owner, "mock-user");
                assert_eq!(repo, "mock-repo");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
