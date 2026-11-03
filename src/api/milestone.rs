use crate::error::{GbMcpError, Result};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};

use super::{client::GitBucketClient, outcome::Operation};
use crate::models::operation::{OperationTarget, ResourceKind};

impl GitBucketClient {
    /// List milestones for a repository.
    pub async fn list_milestones(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<Milestone>> {
        super::url_safety::repository(owner, repo)?;
        let state_param = state.unwrap_or("open");
        self.get_with_query(
            &format!("/repos/{owner}/{repo}/milestones"),
            &[("state", state_param)],
        )
        .await
    }

    /// Get a single milestone.
    pub async fn get_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<Milestone> {
        super::url_safety::repository(owner, repo)?;
        self.get(&format!("/repos/{owner}/{repo}/milestones/{number}"))
            .await
    }

    /// Create a milestone.
    pub async fn create_milestone(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateMilestone,
    ) -> Result<Milestone> {
        super::url_safety::repository(owner, repo)?;
        super::milestone_write::create(self, owner, repo, body).await
    }

    /// Update a milestone while preserving fields omitted by the caller.
    pub async fn update_milestone(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateMilestone,
    ) -> Result<Milestone> {
        super::url_safety::repository(owner, repo)?;
        super::milestone_write::update(self, owner, repo, number, body).await
    }

    /// Delete a milestone.
    pub async fn delete_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        super::url_safety::repository(owner, repo)?;
        let route = self.operation_route(super::profile::Operation::MilestoneDelete);
        let operation = Operation::new(
            "delete_milestone",
            OperationTarget::new(ResourceKind::Milestone, owner, repo).numbered(number),
            route,
            &[("existence", true)],
        );
        operation
            .run(async {
                self.delete(&format!("/repos/{owner}/{repo}/milestones/{number}"))
                    .await?;
                match self.get_milestone(owner, repo, number).await {
                    Err(GbMcpError::Api { status: 404, .. }) => {
                        operation.verify(&[("existence", true)])
                    }
                    Ok(_) => operation.verify(&[("existence", false)]),
                    Err(error) => Err(error),
                }
            })
            .await
    }
}
