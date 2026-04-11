use crate::error::{GbMcpError, Result};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;

impl GitBucketClient {
    /// List milestones for a repository.
    pub async fn list_milestones(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<Milestone>> {
        let state_param = state.unwrap_or("open");
        self.get_paginated(
            &format!("/repos/{owner}/{repo}/milestones"),
            &[("state", state_param)],
        )
        .await
    }

    /// Get a single milestone.
    pub async fn get_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<Milestone> {
        self.get(&format!("/repos/{owner}/{repo}/milestones/{number}"))
            .await
    }

    /// Create a milestone.
    pub async fn create_milestone(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateMilestone,
    ) -> Result<Milestone> {
        match self
            .post(&format!("/repos/{owner}/{repo}/milestones"), body)
            .await
        {
            Ok(milestone) => Ok(milestone),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.create_milestone_with_404_handling(owner, repo, body, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    /// Update a milestone.
    pub async fn update_milestone(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateMilestone,
    ) -> Result<Milestone> {
        match self
            .patch(&format!("/repos/{owner}/{repo}/milestones/{number}"), body)
            .await
        {
            Ok(milestone) => Ok(milestone),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.update_milestone_with_404_handling(owner, repo, number, body, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    /// Delete a milestone.
    pub async fn delete_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        match self
            .delete(&format!("/repos/{owner}/{repo}/milestones/{number}"))
            .await
        {
            Ok(()) => Ok(()),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                self.delete_milestone_with_404_handling(owner, repo, number, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    async fn create_milestone_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateMilestone,
        original_error: GbMcpError,
    ) -> Result<Milestone> {
        match self.get_repository(owner, repo).await {
            Ok(_) => {
                self.create_milestone_via_web_fallback(owner, repo, body)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn update_milestone_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateMilestone,
        original_error: GbMcpError,
    ) -> Result<Milestone> {
        match self.get_milestone(owner, repo, number).await {
            Ok(current) => {
                self.update_milestone_via_web_fallback(owner, repo, number, body, &current)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn delete_milestone_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        original_error: GbMcpError,
    ) -> Result<()> {
        match self.get_milestone(owner, repo, number).await {
            Ok(_) => {
                self.delete_milestone_via_web_fallback(owner, repo, number)
                    .await
            }
            Err(GbMcpError::Api { status: 404, .. }) => Err(original_error),
            Err(err) => Err(err),
        }
    }

    async fn create_milestone_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        body: &CreateMilestone,
    ) -> Result<Milestone> {
        let session = self.web_session().await?;
        let due_date_form = body
            .due_on
            .as_deref()
            .map(to_milestone_form_due_date)
            .transpose()?;
        session
            .create_milestone(
                owner,
                repo,
                &body.title,
                body.description.as_deref(),
                due_date_form.as_deref(),
            )
            .await?;

        let milestones = self.list_milestones(owner, repo, Some("all")).await?;
        milestones
            .into_iter()
            .filter(|milestone| milestone.title == body.title)
            .max_by_key(|milestone| milestone.number)
            .ok_or_else(|| {
                GbMcpError::Other(
                    "Milestone was created via the web UI, but the created milestone could not be fetched from the API."
                        .to_string(),
                )
            })
    }

    async fn update_milestone_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &UpdateMilestone,
        current: &Milestone,
    ) -> Result<Milestone> {
        let current_due_form = current
            .due_on
            .as_deref()
            .map(to_milestone_form_due_date)
            .transpose()?;
        let next_title = body
            .title
            .as_deref()
            .unwrap_or(current.title.as_str())
            .to_string();
        let next_description = body
            .description
            .as_ref()
            .cloned()
            .or_else(|| current.description.clone());
        let next_due_form = match body.due_on.as_deref() {
            Some("") => Some(String::new()),
            Some(value) => Some(to_milestone_form_due_date(value)?),
            None => current_due_form.clone(),
        };

        let needs_content_update = body
            .title
            .as_deref()
            .is_some_and(|title| title != current.title)
            || body.description.as_deref().is_some_and(|description| {
                description != current.description.as_deref().unwrap_or("")
            })
            || body.due_on.as_deref().is_some_and(|due_on| {
                due_on_to_compare_value(due_on) != current_due_form.as_deref().unwrap_or("")
            });
        let state_action = match body.state.as_deref() {
            Some(state) if state != current.state => Some(match state {
                "closed" => "close",
                "open" => "open",
                other => {
                    return Err(GbMcpError::Other(format!(
                        "Web fallback does not support milestone state '{}'",
                        other
                    )));
                }
            }),
            _ => None,
        };

        if !needs_content_update && state_action.is_none() {
            return Ok(current.clone());
        }

        let session = self.web_session().await?;
        if needs_content_update {
            session
                .edit_milestone(
                    owner,
                    repo,
                    number,
                    &next_title,
                    next_description.as_deref(),
                    next_due_form.as_deref().filter(|value| !value.is_empty()),
                )
                .await?;
        }

        if let Some(action) = state_action {
            session
                .update_milestone_state(owner, repo, number, action)
                .await?;
        }

        self.get_milestone(owner, repo, number).await
    }

    async fn delete_milestone_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<()> {
        let session = self.web_session().await?;
        session.delete_milestone(owner, repo, number).await
    }

    async fn web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "This GitBucket instance does not support the REST milestone endpoint. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable web fallback."
                    .to_string(),
            )
        })?;

        GitBucketWebSession::sign_in(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
        )
        .await
    }
}

fn to_milestone_form_due_date(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    if let Some((date, _)) = trimmed.split_once('T') {
        return Ok(date.to_string());
    }

    if trimmed.len() >= 10 && trimmed.as_bytes().get(4) == Some(&b'-') {
        return Ok(trimmed[..10].to_string());
    }

    Ok(trimmed.to_string())
}

fn due_on_to_compare_value(value: &str) -> String {
    to_milestone_form_due_date(value).unwrap_or_else(|_| value.trim().to_string())
}
