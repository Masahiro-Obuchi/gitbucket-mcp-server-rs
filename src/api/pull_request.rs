use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::UpdateIssue;
use crate::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest, UpdatePullRequest,
};

use super::client::GitBucketClient;
use crate::error::GbMcpError;

impl GitBucketClient {
    /// List pull requests for a repository.
    pub async fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<PullRequest>> {
        let state_param = state.unwrap_or("open");
        self.get_paginated(
            &format!("/repos/{}/{}/pulls", owner, repo),
            &[("state", state_param)],
        )
        .await
    }

    /// Get a single pull request.
    pub async fn get_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PullRequest> {
        self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, number))
            .await
    }

    /// Create a new pull request.
    pub async fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        body: &CreatePullRequest,
    ) -> Result<PullRequest> {
        self.post(&format!("/repos/{}/{}/pulls", owner, repo), body)
            .await
    }

    /// Update a pull request (title, body, state, base).
    pub async fn update_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdatePullRequest,
    ) -> Result<PullRequest> {
        match self
            .patch(&format!("/repos/{}/{}/pulls/{}", owner, repo, number), body)
            .await
        {
            Ok(pr) => Ok(pr),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.update_pull_request_with_404_handling(owner, repo, number, body, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    /// Merge a pull request.
    pub async fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &MergePullRequest,
    ) -> Result<MergeResult> {
        self.put(
            &format!("/repos/{}/{}/pulls/{}/merge", owner, repo, number),
            body,
        )
        .await
    }

    /// Add a comment to a pull request (uses the issues comments endpoint).
    pub async fn add_pull_request_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &CreateComment,
    ) -> Result<Comment> {
        self.post(
            &format!("/repos/{}/{}/issues/{}/comments", owner, repo, number),
            body,
        )
        .await
    }

    async fn update_pull_request_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdatePullRequest,
        original_error: GbMcpError,
    ) -> Result<PullRequest> {
        match self.get_pull_request(owner, repo, number).await {
            Ok(_) => {
                self.update_pull_request_via_issue_fallback(owner, repo, number, body)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn update_pull_request_via_issue_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdatePullRequest,
    ) -> Result<PullRequest> {
        if body.base.is_some() {
            return Err(GbMcpError::Other(
                "This GitBucket instance does not support REST pull request updates, and fallback cannot update the base branch.".to_string(),
            ));
        }

        let issue_update = UpdateIssue {
            state: body.state.clone(),
            title: body.title.clone(),
            body: body.body.clone(),
        };
        self.update_issue(owner, repo, number, &issue_update)
            .await?;
        self.get_pull_request(owner, repo, number).await
    }
}
