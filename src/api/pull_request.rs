use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::pull_request::{CreatePullRequest, MergePullRequest, MergeResult, PullRequest};

use super::client::GitBucketClient;

impl GitBucketClient {
    /// List pull requests for a repository.
    pub async fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<PullRequest>> {
        let state_param = state.unwrap_or("open");
        self.get(&format!(
            "/repos/{}/{}/pulls?state={}",
            owner, repo, state_param
        ))
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
}
