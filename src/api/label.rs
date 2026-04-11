use url::Url;

use crate::error::{GbMcpError, Result};
use crate::models::label::{CreateLabel, Label, UpdateLabel};

use super::client::GitBucketClient;

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
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.patch_url(url, &path, body).await
    }

    pub async fn delete_label(&self, owner: &str, repo: &str, name: &str) -> Result<()> {
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.delete_url(url, &path).await
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
