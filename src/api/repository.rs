use crate::error::Result;
use crate::models::repository::{Branch, CreateRepository, Repository};

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::models::operation::{OperationTarget, ResourceKind};

impl GitBucketClient {
    /// List repositories for a user. Falls back to org endpoint on 404.
    pub async fn list_repositories(&self, owner: &str) -> Result<Vec<Repository>> {
        super::url_safety::identifier(owner, "owner")?;
        let result = self
            .get_with_query::<Vec<Repository>>(&format!("/users/{}/repos", owner), &[])
            .await;
        match result {
            Ok(repos) => Ok(repos),
            Err(crate::error::GbMcpError::Api { status: 404, .. }) => {
                self.get_with_query::<Vec<Repository>>(&format!("/orgs/{}/repos", owner), &[])
                    .await
            }
            Err(e) => Err(e),
        }
    }

    /// Get repository details.
    pub async fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        super::url_safety::repository(owner, repo)?;
        self.get(&format!("/repos/{}/{}", owner, repo)).await
    }

    /// Create a new repository for the authenticated user.
    pub async fn create_repository(&self, body: &CreateRepository) -> Result<Repository> {
        let operation = Operation::new(
            "create_repository",
            OperationTarget::new(ResourceKind::Repository, "", &body.name),
            Route::Rest,
            &[],
        );
        operation.run(self.post("/user/repos", body)).await
    }

    /// Fork a repository.
    pub async fn fork_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        super::url_safety::repository(owner, repo)?;
        let operation = Operation::new(
            "fork_repository",
            OperationTarget::new(ResourceKind::Repository, "", repo),
            Route::Rest,
            &[],
        );
        operation
            .run(self.post::<Repository, serde_json::Value>(
                &format!("/repos/{owner}/{repo}/forks"),
                &serde_json::json!({}),
            ))
            .await
    }

    /// List branches for a repository.
    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>> {
        super::url_safety::repository(owner, repo)?;
        self.get_with_query(&format!("/repos/{}/{}/branches", owner, repo), &[])
            .await
    }
}
