use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;
use crate::error::GbMcpError;

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
        match self
            .patch(
                &format!("/repos/{}/{}/issues/{}", owner, repo, number),
                body,
            )
            .await
        {
            Ok(issue) => Ok(issue),
            Err(GbMcpError::Api { status: 404, .. }) => {
                self.update_issue_via_web_fallback(owner, repo, number, body)
                    .await
            }
            Err(err) => Err(err),
        }
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

    async fn update_issue_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
    ) -> Result<Issue> {
        if body.title.is_some() || body.body.is_some() {
            return Err(GbMcpError::Other(
                "This GitBucket instance does not support issue title/body updates via REST. Only state-only updates can fall back to the web UI.".to_string(),
            ));
        }

        let state = body.state.as_deref().ok_or_else(|| {
            GbMcpError::Other(
                "This GitBucket instance does not support issue updates via REST, and web fallback only supports state-only updates.".to_string(),
            )
        })?;

        let action = match state {
            "closed" => "close",
            "open" => "reopen",
            other => {
                return Err(GbMcpError::Other(format!(
                    "Web fallback does not support issue state '{}'",
                    other
                )));
            }
        };

        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "This GitBucket instance does not support REST issue updates. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable state-only web fallback.".to_string(),
            )
        })?;

        let session = GitBucketWebSession::sign_in(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
        )
        .await?;
        session
            .update_issue_state(owner, repo, number, action)
            .await?;
        self.get_issue(owner, repo, number).await
    }
}
