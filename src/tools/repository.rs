use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::repository::CreateRepository;
use crate::server::GitBucketMcpServer;

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
        match self.client.list_repositories(&params.owner).await {
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
        match self
            .client
            .get_repository(&params.owner, &params.repo)
            .await
        {
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
        let body = CreateRepository {
            name: params.name,
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
        match self
            .client
            .fork_repository(&params.owner, &params.repo)
            .await
        {
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
        match self.client.list_branches(&params.owner, &params.repo).await {
            Ok(branches) => serde_json::to_string_pretty(&branches)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}
