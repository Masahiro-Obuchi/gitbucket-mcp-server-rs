use crate::error::{GbMcpError, Result};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;

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
        self.operation_route(super::profile::Operation::MilestoneDelete);
        self.delete(&format!("/repos/{owner}/{repo}/milestones/{number}"))
            .await
    }

    pub(super) async fn milestone_web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "The selected standard milestone route requires GITBUCKET_USERNAME and GITBUCKET_PASSWORD."
                    .to_string(),
            )
        })?;

        GitBucketWebSession::sign_in_with_http_settings(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
            self.http_settings().clone(),
        )
        .await
    }
}
