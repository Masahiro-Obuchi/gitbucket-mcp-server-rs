use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};

use super::client::GitBucketClient;

impl GitBucketClient {
    /// List issues for a repository.
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<Issue>> {
        let state_param = state.unwrap_or("open");
        self.get(&format!(
            "/repos/{}/{}/issues?state={}",
            owner, repo, state_param
        ))
        .await
    }

    /// Get a single issue.
    pub async fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        self.get(&format!("/repos/{}/{}/issues/{}", owner, repo, number))
            .await
    }

    /// Create a new issue.
    pub async fn create_issue(&self, owner: &str, repo: &str, body: &CreateIssue) -> Result<Issue> {
        self.post(&format!("/repos/{}/{}/issues", owner, repo), body)
            .await
    }

    /// Update an issue (title, body, state).
    pub async fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
    ) -> Result<Issue> {
        self.patch(
            &format!("/repos/{}/{}/issues/{}", owner, repo, number),
            body,
        )
        .await
    }

    /// List comments on an issue.
    pub async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Comment>> {
        self.get(&format!(
            "/repos/{}/{}/issues/{}/comments",
            owner, repo, number
        ))
        .await
    }

    /// Add a comment to an issue.
    pub async fn add_issue_comment(
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
