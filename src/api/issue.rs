use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;
use crate::error::GbMcpError;

/// Preserve the stage for callers that must distinguish an unreadable result
/// from a rejected write, even when both errors have the same HTTP status.
pub(super) enum IssueUpdateError {
    BeforeWrite(GbMcpError),
    Write(GbMcpError),
    AfterWrite(GbMcpError),
}

impl IssueUpdateError {
    fn into_cause(self) -> GbMcpError {
        match self {
            Self::BeforeWrite(cause) | Self::Write(cause) | Self::AfterWrite(cause) => cause,
        }
    }
}

type IssueUpdateResult = std::result::Result<Issue, IssueUpdateError>;

impl GitBucketClient {
    /// List issues for a repository.
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<Issue>> {
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
        let issue = self
            .get(&format!("/repos/{}/{}/issues/{}", owner, repo, number))
            .await?;
        Ok(issue_with_closed_at_fallback(issue))
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
        self.update_issue_with_outcome(owner, repo, number, body)
            .await
            .map_err(IssueUpdateError::into_cause)
    }

    pub(super) async fn update_issue_with_outcome(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
    ) -> IssueUpdateResult {
        match self
            .patch(
                &format!("/repos/{}/{}/issues/{}", owner, repo, number),
                body,
            )
            .await
        {
            Ok(issue) => Ok(issue_with_closed_at_fallback(issue)),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.update_issue_with_404_handling(owner, repo, number, body, err)
                    .await
            }
            Err(err) => Err(IssueUpdateError::Write(err)),
        }
    }

    /// List comments on an issue.
    pub async fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Comment>> {
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
    ) -> IssueUpdateResult {
        match self.get_issue(owner, repo, number).await {
            Ok(current_issue) => {
                self.update_issue_via_web_fallback(owner, repo, number, body, &current_issue)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => {
                Err(IssueUpdateError::BeforeWrite(original_error))
            }
            Err(err) => Err(IssueUpdateError::BeforeWrite(err)),
        }
    }

    async fn update_issue_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
        current_issue: &Issue,
    ) -> IssueUpdateResult {
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
                    return Err(IssueUpdateError::BeforeWrite(GbMcpError::Other(format!(
                        "Web fallback does not support issue state '{}'",
                        other
                    ))));
                }
            }),
            _ => None,
        };

        if !needs_title_update && !needs_body_update && state_action.is_none() {
            return Ok(current_issue.clone());
        }

        let credentials = self.web_credentials().ok_or_else(|| {
            IssueUpdateError::BeforeWrite(GbMcpError::Other(
                "This GitBucket instance does not support REST issue updates. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable web fallback.".to_string(),
            ))
        })?;

        let session = GitBucketWebSession::sign_in(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
        )
        .await
        .map_err(IssueUpdateError::BeforeWrite)?;

        // Once a web mutation is attempted, a failure can leave a partial
        // update. Keep that context through later writes and the final read.
        if needs_title_update {
            session
                .edit_issue_title(owner, repo, number, next_title)
                .await
                .map_err(IssueUpdateError::AfterWrite)?;
        }

        if needs_body_update {
            session
                .edit_issue_content(owner, repo, number, next_title, next_body)
                .await
                .map_err(IssueUpdateError::AfterWrite)?;
        }

        if let Some(action) = state_action {
            session
                .update_issue_state(owner, repo, number, action)
                .await
                .map_err(IssueUpdateError::AfterWrite)?;
        }

        self.get_issue(owner, repo, number)
            .await
            .map_err(IssueUpdateError::AfterWrite)
    }
}

fn issue_with_closed_at_fallback(mut issue: Issue) -> Issue {
    if issue.state == "closed" && issue.closed_at.is_none() {
        issue.closed_at = issue.updated_at.clone();
    }
    issue
}
