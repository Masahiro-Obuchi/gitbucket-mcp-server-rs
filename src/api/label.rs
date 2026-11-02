use url::Url;

use crate::error::{GbMcpError, Result};
use crate::models::label::{CreateLabel, Label, UpdateLabel};

use super::client::GitBucketClient;
use super::web::GitBucketWebSession;

impl GitBucketClient {
    pub async fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        super::url_safety::repository(owner, repo)?;
        self.get_with_query(&format!("/repos/{}/{}/labels", owner, repo), &[])
            .await
    }

    pub async fn get_label(&self, owner: &str, repo: &str, name: &str) -> Result<Label> {
        super::url_safety::repository(owner, repo)?;
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.get_url(url, &path).await
    }

    pub async fn create_label(&self, owner: &str, repo: &str, body: &CreateLabel) -> Result<Label> {
        super::url_safety::repository(owner, repo)?;
        super::label_write::name(&body.name)?;
        let request = CreateLabel {
            name: body.name.clone(),
            color: super::label_write::color(&body.color)?,
        };
        let label = self
            .post(&format!("/repos/{}/{}/labels", owner, repo), &request)
            .await?;
        super::label_write::verify(&label, &request.name, &request.color, "Label creation")?;
        Ok(label)
    }

    pub async fn update_label(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
        body: &UpdateLabel,
    ) -> Result<Label> {
        super::url_safety::repository(owner, repo)?;
        super::label_write::update(self, owner, repo, name, body).await
    }

    pub async fn delete_label(&self, owner: &str, repo: &str, name: &str) -> Result<()> {
        super::url_safety::repository(owner, repo)?;
        let path = format!("/repos/{owner}/{repo}/labels/{name}");
        let url = label_url(self, owner, repo, name)?;
        self.delete_url(url, &path).await
    }

    pub(super) async fn label_web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or_else(|| {
            GbMcpError::Other(
                "The standard label color update route requires GITBUCKET_USERNAME and GITBUCKET_PASSWORD."
                    .to_string(),
            )
        })?;

        GitBucketWebSession::sign_in_with_http_settings(
            self.base_url(),
            &credentials.username,
            &credentials.password,
            self.allow_invalid_certs(),
            self.http_settings().clone(),
        )
        .await
    }
}

pub(super) fn label_url(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    name: &str,
) -> Result<Url> {
    super::url_safety::repository(owner, repo)?;
    super::url_safety::append(client.base_url(), &["repos", owner, repo, "labels", name])
}
