use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
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
        let issues = self
            .get_paginated_with_identity(
                &format!("/repos/{}/{}/issues", owner, repo),
                &[("state", state_param)],
                25,
                |issue: &Issue| issue.number,
            )
            .await?;
        Ok(issues
            .into_iter()
            .map(issue_with_closed_at_fallback)
            .collect())
    }

    /// Get a single issue.
    pub async fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        let issue = self
            .get(&format!("/repos/{}/{}/issues/{}", owner, repo, number))
            .await?;
        Ok(issue_with_closed_at_fallback(issue))
    }

    /// Create a new issue.
    pub async fn create_issue(&self, owner: &str, repo: &str, body: &CreateIssue) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        let operation = Operation::new(
            "create_issue",
            OperationTarget::new(ResourceKind::Issue, owner, repo),
            Route::Rest,
            &[],
        );
        operation
            .run(self.post(&format!("/repos/{owner}/{repo}/issues"), body))
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
}

pub(super) fn issue_with_closed_at_fallback(mut issue: Issue) -> Issue {
    if issue.state == "closed" && issue.closed_at.is_none() {
        issue.closed_at = issue.updated_at.clone();
    }
    issue
}
