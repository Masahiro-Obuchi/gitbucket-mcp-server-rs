use url::Url;

use crate::error::{GbMcpError, Result};
use crate::models::label::{CreateLabel, Label, UpdateLabel};
use serde::Serialize;

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;

#[derive(Debug, Serialize)]
struct GitBucketUpdateLabel<'a> {
    name: &'a str,
    color: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

impl GitBucketClient {
    pub async fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        self.get_paginated(&format!("/repos/{}/{}/labels", owner, repo), &[])
            .await
    }

    pub async fn get_label(&self, owner: &str, repo: &str, name: &str) -> Result<Label> {
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.get_url(url, &path).await
    }

    pub async fn create_label(&self, owner: &str, repo: &str, body: &CreateLabel) -> Result<Label> {
        self.post(&format!("/repos/{}/{}/labels", owner, repo), body)
            .await
    }

    pub async fn update_label(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
        body: &UpdateLabel,
    ) -> Result<Label> {
        // Only fetch the current label when we need it to fill in missing fields.
        let current_opt: Option<Label> = if body.new_name.is_none() || body.color.is_none() {
            Some(self.get_label(owner, repo, name).await?)
        } else {
            None
        };

        let next_name = body
            .new_name
            .as_deref()
            .or_else(|| current_opt.as_ref().map(|c| c.name.as_str()))
            .unwrap_or(name)
            .to_string();
        let next_color = body
            .color
            .as_deref()
            .or_else(|| current_opt.as_ref().and_then(|c| c.color.as_deref()))
            .ok_or_else(|| {
                GbMcpError::Other(
                    "The current label color could not be fetched for label update.".to_string(),
                )
            })?
            .trim_start_matches('#')
            .to_string();
        let request = GitBucketUpdateLabel {
            name: &next_name,
            color: &next_color,
            description: body.description.as_deref(),
        };
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        match self.patch_url(url, &path, &request).await {
            Ok(label) => Ok(label),
            Err(err @ GbMcpError::Api { status: 404, .. }) => {
                // Lazily fetch current label for the web fallback if we skipped it above.
                let current = match current_opt {
                    Some(c) => c,
                    None => self.get_label(owner, repo, name).await?,
                };
                self.update_label_with_404_handling(owner, repo, body, &current, err)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    pub async fn delete_label(&self, owner: &str, repo: &str, name: &str) -> Result<()> {
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.delete_url(url, &path).await
    }

    async fn update_label_with_404_handling(
        &self,
        owner: &str,
        repo: &str,
        body: &UpdateLabel,
        current: &Label,
        _original_error: GbMcpError,
    ) -> Result<Label> {
        self.update_label_via_web_fallback(owner, repo, body, current)
            .await
    }

    async fn update_label_via_web_fallback(
        &self,
        owner: &str,
        repo: &str,
        body: &UpdateLabel,
        current: &Label,
    ) -> Result<Label> {
        let next_name = body
            .new_name
            .as_deref()
            .unwrap_or(current.name.as_str())
            .to_string();
        let next_color = body
            .color
            .as_deref()
            .or(current.color.as_deref())
            .ok_or_else(|| {
                GbMcpError::Other(
                    "This GitBucket instance does not support the REST label update endpoint, and the current label color could not be fetched for web fallback."
                        .to_string(),
                )
            })?
            .trim_start_matches('#')
            .to_string();

        let needs_update = body
            .new_name
            .as_deref()
            .is_some_and(|name| name != current.name)
            || body.color.as_deref().is_some_and(|color| {
                color.trim_start_matches('#') != current.color.as_deref().unwrap_or("")
            });

        if !needs_update {
            if body.description.is_some() {
                return Err(GbMcpError::Other(
                    "This GitBucket instance does not support label description-only updates."
                        .to_string(),
                ));
            }
            return Ok(current.clone());
        }

        let session = self.label_web_session().await?;
        let label_id = session.find_label_id(owner, repo, &current.name).await?;
        session
            .edit_label(owner, repo, label_id, &next_name, &next_color)
            .await?;

        self.get_label(owner, repo, &next_name).await
    }

    async fn label_web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "This GitBucket instance does not support the REST label update endpoint. Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD to enable web fallback."
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

fn label_url(client: &GitBucketClient, owner: &str, repo: &str, name: &str) -> Result<Url> {
    let mut url = Url::parse(client.base_url()).map_err(GbMcpError::UrlParse)?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            GbMcpError::Other("GitBucket base URL cannot be used as a path base".to_string())
        })?;
        segments.pop_if_empty();
        segments.extend(["repos", owner, repo, "labels", name]);
    }
    Ok(url)
}
