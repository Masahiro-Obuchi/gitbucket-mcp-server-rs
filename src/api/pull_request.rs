use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest, UpdatePullRequest,
};

use super::client::GitBucketClient;

impl GitBucketClient {
    /// List pull requests for a repository.
    pub async fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<PullRequest>> {
        super::url_safety::repository(owner, repo)?;
        let state_param = state.unwrap_or("open");
        self.get_paginated_with_identity(
            &format!("/repos/{}/{}/pulls", owner, repo),
            &[("state", state_param)],
            25,
            |pr: &PullRequest| pr.number,
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
        super::url_safety::repository(owner, repo)?;
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
        super::url_safety::repository(owner, repo)?;
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
        super::url_safety::repository(owner, repo)?;
        super::pull_request_update::update(self, owner, repo, number, body).await
    }

    /// Merge a pull request.
    pub async fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &MergePullRequest,
    ) -> Result<MergeResult> {
        super::url_safety::repository(owner, repo)?;
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
        super::url_safety::repository(owner, repo)?;
        self.post(
            &format!("/repos/{}/{}/issues/{}/comments", owner, repo, number),
            body,
        )
        .await
    }
}
