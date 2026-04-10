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
        self.get_paginated(
            &format!("/repos/{}/{}/issues", owner, repo),
            &[("state", state_param)],
        )
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
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.update_issue_with_404_handling(owner, repo, number, body, err)
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
        self.get_paginated(
            &format!("/repos/{}/{}/issues/{}/comments", owner, repo, number),
            &[],
        )
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

    async fn update_issue_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
        original_error: GbMcpError,
    ) -> Result<Issue> {
        match self.get_issue(owner, repo, number).await {
            Ok(current_issue) => {
                self.update_issue_via_web_fallback(owner, repo, number, body, &current_issue)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn update_issue_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
        current_issue: &Issue,
    ) -> Result<Issue> {
        let current_body = current_issue.body.as_deref().unwrap_or_default();
        let next_title = body
            .title
            .as_deref()
            .unwrap_or(current_issue.title.as_str());
        let next_body = body.body.as_deref().unwrap_or(current_body);

        let needs_title_update = body
            .title
            .as_deref()
            .is_some_and(|title| title != current_issue.title);
        let needs_body_update = body
            .body
            .as_deref()
            .is_some_and(|content| content != current_body);
        let state_action = match body.state.as_deref() {
            Some(state) if state != current_issue.state => Some(match state {
                "closed" => "close",
                "open" => "reopen",
                other => {
                    return Err(GbMcpError::Other(format!(
                        "Web fallback does not support issue state '{}'",
                        other
                    )));
                }
            }),
            _ => None,
        };

        if !needs_title_update && !needs_body_update && state_action.is_none() {
            return Ok(current_issue.clone());
        }

        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "This GitBucket instance does not support REST issue updates. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable web fallback.".to_string(),
            )
        })?;

        let session = GitBucketWebSession::sign_in(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
        )
        .await?;

        if needs_title_update {
            session
                .edit_issue_title(owner, repo, number, next_title)
                .await?;
        }

        if needs_body_update {
            session
                .edit_issue_content(owner, repo, number, next_title, next_body)
                .await?;
        }

        if let Some(action) = state_action {
            session
                .update_issue_state(owner, repo, number, action)
                .await?;
        }

        self.get_issue(owner, repo, number).await
    }
}
