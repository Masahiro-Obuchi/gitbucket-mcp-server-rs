use reqwest::redirect::Policy;
use reqwest::{Client, Response, StatusCode};

use crate::error::{GbMcpError, Result};

#[derive(Debug, Clone)]
pub struct GitBucketWebSession {
    client: Client,
    base_url: String,
}

impl GitBucketWebSession {
    pub async fn sign_in(
        api_base_url: &str,
        username: &str,
        password: &str,
        allow_invalid_certs: bool,
    ) -> Result<Self> {
        let base_url = normalize_web_base_url(api_base_url);
        let client = Client::builder()
            .cookie_store(true)
            .redirect(Policy::limited(10))
            .danger_accept_invalid_certs(allow_invalid_certs)
            .build()
            .map_err(GbMcpError::Http)?;

        let response = client
            .post(format!("{base_url}/signin"))
            .form(&[("userName", username), ("password", password), ("hash", "")])
            .send()
            .await
            .map_err(GbMcpError::Http)?;

        let status = response.status();
        let final_path = response.url().path().to_string();

        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || final_path.ends_with("/signin")
        {
            return Err(GbMcpError::Other(format!(
                "GitBucket web sign-in failed for '{}'. Check GITBUCKET_USERNAME and GITBUCKET_PASSWORD.",
                username
            )));
        }

        if !status.is_success() && !status.is_redirection() {
            return Err(GbMcpError::Other(format!(
                "GitBucket web sign-in failed: HTTP {}",
                status.as_u16()
            )));
        }

        Ok(Self { client, base_url })
    }

    pub async fn update_issue_state(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        action: &str,
    ) -> Result<()> {
        self.post_form(
            &format!("/{owner}/{repo}/issue_comments/state"),
            vec![
                ("issueId", number.to_string()),
                ("content", String::new()),
                ("action", action.to_string()),
            ],
            "change the issue state",
        )
        .await
    }

    pub async fn edit_issue_title(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
    ) -> Result<()> {
        self.post_form(
            &format!("/{owner}/{repo}/issues/edit_title/{number}"),
            vec![("title", title.to_string())],
            "edit the issue title",
        )
        .await
    }

    pub async fn edit_issue_content(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        content: &str,
    ) -> Result<()> {
        self.post_form(
            &format!("/{owner}/{repo}/issues/edit/{number}"),
            vec![
                ("title", title.to_string()),
                ("content", content.to_string()),
            ],
            "edit the issue",
        )
        .await
    }

    pub async fn create_milestone(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        description: Option<&str>,
        due_date: Option<&str>,
    ) -> Result<()> {
        self.post_form(
            &format!("/{owner}/{repo}/issues/milestones/new"),
            vec![
                ("title", title.to_string()),
                ("description", description.unwrap_or_default().to_string()),
                ("dueDate", due_date.unwrap_or_default().to_string()),
            ],
            "create the milestone",
        )
        .await
    }

    pub async fn edit_milestone(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        description: Option<&str>,
        due_date: Option<&str>,
    ) -> Result<()> {
        self.post_form(
            &format!("/{owner}/{repo}/issues/milestones/{number}/edit"),
            vec![
                ("title", title.to_string()),
                ("description", description.unwrap_or_default().to_string()),
                ("dueDate", due_date.unwrap_or_default().to_string()),
            ],
            "edit the milestone",
        )
        .await
    }

    pub async fn update_milestone_state(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        state: &str,
    ) -> Result<()> {
        let action = match state {
            "open" | "close" => state,
            other => {
                return Err(GbMcpError::Other(format!(
                    "Invalid milestone state action '{}'. Expected open or close.",
                    other
                )));
            }
        };

        let response = self
            .client
            .get(format!(
                "{}/{owner}/{repo}/issues/milestones/{number}/{action}",
                self.base_url
            ))
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.ensure_success(response, "change the milestone state")
            .await
    }

    pub async fn delete_milestone(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        let response = self
            .client
            .get(format!(
                "{}/{owner}/{repo}/issues/milestones/{number}/delete",
                self.base_url
            ))
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.ensure_success(response, "delete the milestone").await
    }

    async fn post_form(&self, path: &str, fields: Vec<(&str, String)>, action: &str) -> Result<()> {
        let response = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .form(&fields)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.ensure_success(response, action).await
    }

    async fn ensure_success(&self, response: Response, action: &str) -> Result<()> {
        let status = response.status();
        let final_path = response.url().path().to_string();
        let body = response.text().await.unwrap_or_default();

        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || final_path.ends_with("/signin")
        {
            return Err(GbMcpError::Other(format!(
                "GitBucket web session failed while trying to {}. Check GITBUCKET_USERNAME and GITBUCKET_PASSWORD.",
                action
            )));
        }

        if status.is_success() || status.is_redirection() {
            return Ok(());
        }

        let suffix = if body.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", body.trim())
        };
        Err(GbMcpError::Other(format!(
            "Failed to {}: HTTP {}{}",
            action,
            status.as_u16(),
            suffix
        )))
    }
}

fn normalize_web_base_url(api_base_url: &str) -> String {
    api_base_url.trim_end_matches("/api/v3").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_web_base_url() {
        assert_eq!(
            normalize_web_base_url("https://example.com/gitbucket/api/v3"),
            "https://example.com/gitbucket"
        );
    }
}
