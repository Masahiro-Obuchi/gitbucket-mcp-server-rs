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
        match self
            .delete(&format!("/repos/{owner}/{repo}/milestones/{number}"))
            .await
        {
            Ok(()) => Ok(()),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.delete_milestone_with_404_handling(owner, repo, number, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    async fn delete_milestone_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        original_error: GbMcpError,
    ) -> Result<()> {
        super::url_safety::repository(owner, repo)?;
        match self.get_milestone(owner, repo, number).await {
            Ok(_) => {
                self.delete_milestone_via_web_fallback(owner, repo, number)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn delete_milestone_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<()> {
        super::url_safety::repository(owner, repo)?;
        let session = self.milestone_web_session().await?;
        session.delete_milestone(owner, repo, number).await
    }

    pub(super) async fn milestone_web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "The REST milestone request could not be completed. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable web compatibility fallback; HTTP 404 alone does not establish that the REST endpoint is unavailable."
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
