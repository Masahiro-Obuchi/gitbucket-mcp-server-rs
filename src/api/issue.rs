use crate::error::Result;
use crate::models::comment::{Comment, CreateComment, UpdateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::models::operation::{OperationTarget, ResourceKind};

impl GitBucketClient {
    /// List issues for a repository.
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<Issue>> {
        super::url_safety::repository(owner, repo)?;
        let state_param = state.unwrap_or("open");
        self.get_paginated_with_identity(
            &format!("/repos/{}/{}/issues", owner, repo),
            &[("state", state_param)],
            25,
            |issue: &Issue| issue.number,
        )
        .await
    }

    /// Get a single issue.
    pub async fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        self.get(&format!("/repos/{}/{}/issues/{}", owner, repo, number))
            .await
    }

    /// Create a new issue.
    pub async fn create_issue(&self, owner: &str, repo: &str, body: &CreateIssue) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        super::creation::issue(self, owner, repo, body).await
    }

    /// Update an issue (title, body, state).
    pub async fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
    ) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        super::issue_update::update(self, owner, repo, number, body).await
    }

    /// List comments on an issue.
    pub async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Comment>> {
        super::url_safety::repository(owner, repo)?;
        self.get_with_query(
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
        super::url_safety::repository(owner, repo)?;
        let operation = Operation::new(
            "add_issue_comment",
            OperationTarget::new(ResourceKind::IssueComment, owner, repo).parented(number),
            Route::Rest,
            &[],
        );
        operation
            .run(self.post(
                &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
                body,
            ))
            .await
    }

    /// Get one conversation comment (including comments attached to a PR).
    pub async fn get_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
    ) -> Result<Comment> {
        super::url_safety::repository(owner, repo)?;
        validate_comment_id(comment_id)?;
        let comment: Comment = self
            .get(&format!(
                "/repos/{owner}/{repo}/issues/comments/{comment_id}"
            ))
            .await?;
        if comment.id != comment_id {
            return Err(crate::error::GbMcpError::Other(
                "GitBucket returned a different comment ID".into(),
            ));
        }
        Ok(comment)
    }

    /// Replace a conversation comment body and verify the exact stored body.
    pub async fn update_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &UpdateComment,
    ) -> Result<Comment> {
        super::url_safety::repository(owner, repo)?;
        validate_comment_id(comment_id)?;
        let operation = Operation::new(
            "update_issue_comment",
            OperationTarget::new(ResourceKind::IssueComment, owner, repo).numbered(comment_id),
            Route::Rest,
            &[("body", true)],
        );
        operation
            .run(async {
                // The native PATCH accepts system timeline entries that its
                // conversation-comment GET deliberately hides. Establish the
                // supported target before sending a potentially saved edit.
                self.get_issue_comment(owner, repo, comment_id).await?;
                let path = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
                let updated: Comment = self.patch(&path, body).await?;
                operation.identity(updated.id)?;
                let observed = self.get_issue_comment(owner, repo, comment_id).await?;
                let body_matches = observed.body.as_deref() == Some(body.body.as_str());
                operation.verify(&[("body", body_matches)])?;
                Ok(observed)
            })
            .await
    }

    /// Delete one conversation comment and verify it can no longer be fetched.
    pub async fn delete_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
    ) -> Result<()> {
        super::url_safety::repository(owner, repo)?;
        validate_comment_id(comment_id)?;
        let operation = Operation::new(
            "delete_issue_comment",
            OperationTarget::new(ResourceKind::IssueComment, owner, repo).numbered(comment_id),
            Route::Rest,
            &[("existence", true)],
        );
        operation
            .run(async {
                self.get_issue_comment(owner, repo, comment_id).await?;
                let path = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
                let sent = self.delete_comment_once(&path, comment_id).await;
                // A 4xx different from this ID is a definite REST rejection;
                // http::mutation already treats HTTP 408 as uncertain.
                // Protocol errors (e.g. HTTP 001) and ID-shaped statuses can
                // follow deletion, so inspect the effect without a second write.
                if let Err(crate::error::GbMcpError::Api { status: 400..=499, .. }) = &sent {
                    return sent;
                }
                match self.get_issue_comment(owner, repo, comment_id).await {
                    Err(crate::error::GbMcpError::Api { status: 404, .. }) => {
                        let visible = self.get_repository(owner, repo).await?;
                        if visible.full_name != format!("{owner}/{repo}") {
                            return Err(crate::error::GbMcpError::Other("Repository visibility could not be verified after comment deletion".into()));
                        }
                        operation.verify(&[("existence", true)])
                    }
                    Ok(_) => {
                        sent?;
                        operation.verify(&[("existence", false)])
                    },
                    Err(error) => Err(error),
                }
            })
            .await
    }
}

pub(crate) fn validate_comment_id(comment_id: u64) -> Result<()> {
    if comment_id == 0 || comment_id > i32::MAX as u64 {
        return Err(crate::error::GbMcpError::InvalidInput(
            "comment_id must be between 1 and 2147483647".into(),
        ));
    }
    Ok(())
}
