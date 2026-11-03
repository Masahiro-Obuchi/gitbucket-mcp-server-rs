use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};

use super::client::GitBucketClient;
use crate::error::GbMcpError;

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
        super::url_safety::repository(owner, repo)?;
        if self.operation_route(super::profile::Operation::IssueUpdate)
            == super::profile::Route::Web
        {
            let current = self.get_issue(owner, repo, number).await?;
            self.update_issue_via_web(owner, repo, number, body, &current)
                .await
        } else {
            self.patch(&format!("/repos/{owner}/{repo}/issues/{number}"), body)
                .await
                .map(issue_with_closed_at_fallback)
        }
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
        self.post(
            &format!("/repos/{}/{}/issues/{}/comments", owner, repo, number),
            body,
        )
        .await
    }

    async fn update_issue_via_web(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateIssue,
        current_issue: &Issue,
    ) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        if current_issue.number != number {
            return Err(GbMcpError::Other(
                "GitBucket returned a different issue; no write was sent".into(),
            ));
        }
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

        let session = self.web_session().await?;

        // Once a web mutation is attempted, a failure can leave a partial
        // update. Keep that context through later writes and the final read.
        if needs_title_update {
            session
                .edit_issue_title(owner, repo, number, next_title)
                .await?;
            let actual = self
                .get_issue(owner, repo, number)
                .await
                .map_err(|error| super::http::unknown("Issue update", error))?;
            verify_web_issue(&actual, number, Some(next_title), None, None)?;
        }

        if needs_body_update {
            session
                .edit_issue_content(owner, repo, number, next_body)
                .await
                .map_err(|error| {
                    if needs_title_update {
                        super::http::unknown("Issue update", error)
                    } else {
                        error
                    }
                })?;
            let actual = self
                .get_issue(owner, repo, number)
                .await
                .map_err(|error| super::http::unknown("Issue update", error))?;
            verify_web_issue(
                &actual,
                number,
                body.title.as_deref(),
                Some(next_body),
                None,
            )?;
        }

        if let Some(action) = state_action {
            session
                .update_issue_state(owner, repo, number, action)
                .await
                .map_err(|error| {
                    if needs_title_update || needs_body_update {
                        super::http::unknown("Issue update", error)
                    } else {
                        error
                    }
                })?;
        }

        let actual = self
            .get_issue(owner, repo, number)
            .await
            .map_err(|error| super::http::unknown("Issue update", error))?;
        verify_web_issue(
            &actual,
            number,
            body.title.as_deref(),
            body.body.as_deref(),
            body.state.as_deref(),
        )?;
        Ok(actual)
    }
}

fn verify_web_issue(
    actual: &Issue,
    number: u64,
    title: Option<&str>,
    body: Option<&str>,
    state: Option<&str>,
) -> Result<()> {
    let mut fields = Vec::new();
    if actual.number != number {
        fields.push("number");
    }
    if title.is_some_and(|v| v != actual.title) {
        fields.push("title");
    }
    if body.is_some_and(|v| v != actual.body.as_deref().unwrap_or_default()) {
        fields.push("body");
    }
    if state.is_some_and(|v| v != actual.state) {
        fields.push("state");
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(GbMcpError::OperationNotVerified {
            operation: "Issue update",
            fields,
        })
    }
}

fn issue_with_closed_at_fallback(mut issue: Issue) -> Issue {
    if issue.state == "closed" && issue.closed_at.is_none() {
        issue.closed_at = issue.updated_at.clone();
    }
    issue
}
