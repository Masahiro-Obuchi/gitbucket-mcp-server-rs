use url::form_urlencoded::byte_serialize;

use crate::error::Result;
use crate::models::label::{CreateLabel, Label};

use super::client::GitBucketClient;

impl GitBucketClient {
    pub async fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        self.get_paginated(&format!("/repos/{}/{}/labels", owner, repo), &[])
            .await
    }

    pub async fn get_label(&self, owner: &str, repo: &str, name: &str) -> Result<Label> {
        self.get(&format!(
            "/repos/{}/{}/labels/{}",
            owner,
            repo,
            encode_path_segment(name)
        ))
        .await
    }

    pub async fn create_label(&self, owner: &str, repo: &str, body: &CreateLabel) -> Result<Label> {
        self.post(&format!("/repos/{}/{}/labels", owner, repo), body)
            .await
    }

    pub async fn delete_label(&self, owner: &str, repo: &str, name: &str) -> Result<()> {
        self.delete(&format!(
            "/repos/{}/{}/labels/{}",
            owner,
            repo,
            encode_path_segment(name)
        ))
        .await
    }
}

fn encode_path_segment(value: &str) -> String {
    byte_serialize(value.as_bytes()).collect()
}
