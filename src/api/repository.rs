use crate::error::Result;
use crate::models::repository::{Branch, CreateRepository, Repository};

use super::client::GitBucketClient;

impl GitBucketClient {
    /// List repositories for a user. Falls back to org endpoint on 404.
    pub async fn list_repositories(&self, owner: &str) -> Result<Vec<Repository>> {
        let result = self
            .get_paginated::<Repository>(&format!("/users/{}/repos", owner), &[])
            .await;
        match result {
            Ok(repos) => Ok(repos),
            Err(crate::error::GbMcpError::Api { status: 404, .. }) => {
                self.get_paginated::<Repository>(&format!("/orgs/{}/repos", owner), &[])
                    .await
            }
            Err(e) => Err(e),
        }
    }

    /// Get repository details.
    pub async fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        self.get(&format!("/repos/{}/{}", owner, repo)).await
    }

    /// Create a new repository for the authenticated user.
    pub async fn create_repository(&self, body: &CreateRepository) -> Result<Repository> {
        self.post("/user/repos", body).await
    }

    /// Fork a repository.
    pub async fn fork_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        self.post::<Repository, serde_json::Value>(
            &format!("/repos/{}/{}/forks", owner, repo),
            &serde_json::json!({}),
        )
        .await
    }

    /// List branches for a repository.
    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>> {
        self.get_paginated(&format!("/repos/{}/{}/branches", owner, repo), &[])
            .await
    }
}
