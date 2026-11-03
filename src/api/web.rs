use super::{
    client::GitBucketClient,
    http::{self, HttpSettings},
    url_safety, web_html,
};
use crate::error::{GbMcpError, Result};
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use url::Url;

#[derive(Clone)]
pub struct GitBucketWebSession {
    client: Client,
    api_base: Url,
    base_url: String,
    login: String,
    http_settings: HttpSettings,
}

#[derive(Deserialize)]
struct Identity {
    login: String,
}

impl GitBucketClient {
    /// Establish a cookie-only session belonging to the PAT's authenticated user.
    pub async fn web_session(&self) -> Result<GitBucketWebSession> {
        let credentials = self.web_credentials().ok_or(GbMcpError::Authentication(
            "Web operations require GITBUCKET_USERNAME and GITBUCKET_PASSWORD",
        ))?;
        let user = self.get_authenticated_user().await?;
        url_safety::identifier(&user.login, "authenticated user")
            .map_err(|_| GbMcpError::Authentication("REST user identity is invalid"))?;
        if user.login != credentials.username {
            return Err(GbMcpError::Authentication("REST token and configured web username do not identify the same account; no web sign-in or resource write was sent"));
        }
        GitBucketWebSession::connect(self, &credentials.username, &credentials.password).await
    }
}

#[derive(Clone)]
enum Reply {
    IssueData { path: String },
    Page { paths: Vec<String> },
    Label { id: u64, name: String, issues: Url },
    Milestones { path: String, new_url: Url },
}

impl GitBucketWebSession {
    async fn connect(api: &GitBucketClient, username: &str, password: &str) -> Result<Self> {
        let api_base = Url::parse(api.base_url())?;
        let base_url = api
            .base_url()
            .strip_suffix("/api/v3")
            .expect("normalized API suffix")
            .to_string();
        // Never let the HTTP client replay a credential/form POST on a redirect.
        let client = api
            .http_settings()
            .configure(Client::builder())
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(api.allow_invalid_certs())
            .build()
            .map_err(GbMcpError::from)?;
        let session = Self {
            client,
            api_base,
            base_url,
            login: username.into(),
            http_settings: api.http_settings().clone(),
        };
        let mut response = session
            .client
            .post(url_safety::append(&session.base_url, &["signin"])?)
            .form(&[("userName", username), ("password", password), ("hash", "")])
            .send()
            .await
            .map_err(GbMcpError::from)?;
        for _ in 0..10 {
            let status = response.status();
            if matches!(
                status,
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
            ) {
                let target = session.location(&response)?;
                let relative = target
                    .path()
                    .strip_prefix(Url::parse(&format!("{}/", session.base_url))?.path())
                    .unwrap_or("");
                if !(relative.is_empty()
                    || relative == "dashboard"
                    || relative.starts_with("dashboard/"))
                    || target.query().is_some()
                {
                    return Err(GbMcpError::Authentication(
                        "Web sign-in did not lead to a recognized authenticated landing page",
                    ));
                }
                response = session
                    .client
                    .get(target)
                    .send()
                    .await
                    .map_err(GbMcpError::from)?;
                continue;
            }
            if !status.is_success() {
                return Err(GbMcpError::Authentication(
                    "GitBucket web sign-in failed or requested an unsupported authentication flow",
                ));
            }
            let page_url = response.url().clone();
            let body = http::bounded_text(response, session.http_settings.html_limit_bytes, "HTML")
                .await?;
            web_html::check_page(&body, &page_url, &session.api_base)?;
            session.check_identity().await?;
            return Ok(session);
        }
        Err(GbMcpError::Authentication(
            "Web sign-in exceeded its redirect limit",
        ))
    }

    fn location(&self, response: &Response) -> Result<Url> {
        let mut target = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| response.url().join(value).ok())
            .ok_or_else(|| {
                GbMcpError::Other("GitBucket returned an invalid redirect destination".into())
            })?;
        // Jetty may rewrite the first login redirect before it sees the cookie.
        // Keep only the cookie session; never send session IDs in request URLs.
        let path = target
            .path()
            .split('/')
            .map(|segment| {
                if let Some((path, id)) = segment.split_once(";jsessionid=") {
                    if id.is_empty()
                        || !id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                        || path.contains(';')
                    {
                        return Err(GbMcpError::Authentication(
                            "Invalid URL session parameter in GitBucket redirect",
                        ));
                    }
                    Ok(path)
                } else if segment.contains(';') {
                    Err(GbMcpError::Authentication(
                        "Unsupported URL session parameter in GitBucket redirect",
                    ))
                } else {
                    Ok(segment)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join("/");
        target.set_path(&path);
        url_safety::validate_redirect(&self.api_base, &target)?;
        Ok(target)
    }

    async fn check_identity(&self) -> Result<()> {
        let response = self
            .client
            .get(url_safety::append(self.api_base.as_str(), &["user"])?)
            .send()
            .await
            .map_err(GbMcpError::from)?;
        if response.status() != StatusCode::OK {
            return Err(GbMcpError::Authentication("Cookie-only GitBucket identity could not be verified; no new resource write was sent"));
        }
        let body =
            http::bounded_text(response, self.http_settings.json_limit_bytes, "JSON").await?;
        let identity: Identity = serde_json::from_str(&body).map_err(|_| {
            GbMcpError::Authentication("GitBucket cookie identity response was not a user object")
        })?;
        if identity.login != self.login {
            return Err(GbMcpError::Authentication(
                "REST and web session identities differ; no new resource write was sent",
            ));
        }
        Ok(())
    }

    async fn response(
        &self,
        response: Response,
        reply: &Reply,
        redirected: bool,
    ) -> Result<String> {
        if !response.status().is_success() {
            if matches!(
                response.status(),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ) {
                return Err(GbMcpError::Authentication(
                    "GitBucket web session no longer authorizes this operation",
                ));
            }
            return Err(GbMcpError::Other(format!(
                "GitBucket web request failed: HTTP {}",
                response.status().as_u16()
            )));
        }
        let (limit, kind) = if matches!(reply, Reply::IssueData { .. }) {
            (self.http_settings.json_limit_bytes, "JSON")
        } else {
            (self.http_settings.html_limit_bytes, "HTML")
        };
        let page_url = response.url().clone();
        let body = http::bounded_text(response, limit, kind).await?;
        if !matches!(reply, Reply::IssueData { .. }) {
            web_html::check_page(&body, &page_url, &self.api_base)?;
        }
        match reply {
            Reply::IssueData { .. } => {
                if !redirected {
                    return Err(GbMcpError::Other(
                        "GitBucket returned an issue form instead of its success redirect".into(),
                    ));
                }
                #[derive(Deserialize)]
                struct IssueData {
                    title: String,
                    content: String,
                }
                let value: IssueData = serde_json::from_str(&body).map_err(|_| {
                    GbMcpError::Other(
                        "GitBucket did not return the expected issue data object".into(),
                    )
                })?;
                let _ = (value.title, value.content);
            }
            Reply::Label { id, name, issues } => {
                if web_html::label_id(&body, issues, name)? != *id {
                    return Err(GbMcpError::Other(
                        "GitBucket returned a different label row after writing".into(),
                    ));
                }
            }
            Reply::Milestones { new_url, .. } => {
                if !redirected {
                    return Err(GbMcpError::Other(
                        "GitBucket returned a milestone form instead of its success redirect"
                            .into(),
                    ));
                }
                web_html::milestone_list(&body, new_url)?;
            }
            Reply::Page { .. } => {
                if !redirected {
                    return Err(GbMcpError::Other(
                        "GitBucket did not return the expected issue state redirect".into(),
                    ));
                }
            }
        }
        Ok(body)
    }

    async fn write(
        &self,
        builder: reqwest::RequestBuilder,
        reply: Reply,
        operation: &'static str,
    ) -> Result<()> {
        let request = builder.build().map_err(GbMcpError::from)?;
        // Auth failures here precede this mutation, while the caller preserves
        // any previous mutations in a multi-step operation.
        self.check_identity().await?;
        http::mutation(operation, async {
            let response = self.client.execute(request).await.map_err(GbMcpError::from)?;
            if matches!(
                response.status(),
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
            ) {
                let target = self.location(&response)?;
                let paths: Vec<&str> = match &reply {
                    Reply::IssueData { path } | Reply::Milestones { path, .. } => vec![path],
                    Reply::Page { paths } => paths.iter().map(String::as_str).collect(),
                    Reply::Label { .. } => Vec::new(),
                };
                if target.query().is_some() || !paths.contains(&target.path()) {
                    return Err(GbMcpError::Other("GitBucket redirected a write to an unexpected destination; no follow-up request was sent".into()));
                }
                let read = self.client.get(target).send().await.map_err(GbMcpError::from)?;
                self.response(read, &reply, true).await?;
            } else if response.status().is_redirection() {
                return Err(GbMcpError::Other("GitBucket returned a write redirect that would require replay or has no verified contract; the write was not replayed".into()));
            } else {
                self.response(response, &reply, false).await?;
            }
            Ok(())
        })
        .await
    }

    fn repo_url(&self, owner: &str, repo: &str, tail: &[&str]) -> Result<Url> {
        url_safety::repository(owner, repo)?;
        let mut segments = vec![owner, repo];
        segments.extend_from_slice(tail);
        url_safety::append(&self.base_url, &segments)
    }

    pub async fn edit_issue_title(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
    ) -> Result<()> {
        let url = self.repo_url(owner, repo, &["issues", "edit_title", &number.to_string()])?;
        let data = self.repo_url(owner, repo, &["issues", "_data", &number.to_string()])?;
        self.write(
            self.client.post(url).form(&[("title", title)]),
            Reply::IssueData {
                path: data.path().into(),
            },
            "Issue title update",
        )
        .await
    }
    pub async fn edit_issue_content(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        content: &str,
    ) -> Result<()> {
        let url = self.repo_url(owner, repo, &["issues", "edit", &number.to_string()])?;
        let data = self.repo_url(owner, repo, &["issues", "_data", &number.to_string()])?;
        self.write(
            self.client.post(url).form(&[("content", content)]),
            Reply::IssueData {
                path: data.path().into(),
            },
            "Issue body update",
        )
        .await
    }
    pub async fn update_issue_state(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        action: &str,
    ) -> Result<()> {
        if !matches!(action, "close" | "reopen") {
            return Err(GbMcpError::InvalidInput(
                "Invalid Issue state action".into(),
            ));
        }
        let url = self.repo_url(owner, repo, &["issue_comments", "state"])?;
        let paths = ["issues", "pull"]
            .into_iter()
            .map(|kind| {
                self.repo_url(owner, repo, &[kind, &number.to_string()])
                    .map(|url| url.path().into())
            })
            .collect::<Result<Vec<_>>>()?;
        self.write(
            self.client.post(url).form(&[
                ("issueId", number.to_string()),
                ("content", String::new()),
                ("action", action.into()),
            ]),
            Reply::Page { paths },
            "Issue state update",
        )
        .await
    }
    fn milestone_reply(&self, owner: &str, repo: &str) -> Result<Reply> {
        Ok(Reply::Milestones {
            path: self
                .repo_url(owner, repo, &["issues", "milestones"])?
                .path()
                .into(),
            new_url: self.repo_url(owner, repo, &["issues", "milestones", "new"])?,
        })
    }
    pub async fn create_milestone(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        description: Option<&str>,
        due_date: Option<&str>,
    ) -> Result<()> {
        let url = self.repo_url(owner, repo, &["issues", "milestones", "new"])?;
        self.write(
            self.client.post(url).form(&[
                ("title", title),
                ("description", description.unwrap_or_default()),
                ("dueDate", due_date.unwrap_or_default()),
            ]),
            self.milestone_reply(owner, repo)?,
            "Milestone creation",
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
        let url = self.repo_url(
            owner,
            repo,
            &["issues", "milestones", &number.to_string(), "edit"],
        )?;
        self.write(
            self.client.post(url).form(&[
                ("title", title),
                ("description", description.unwrap_or_default()),
                ("dueDate", due_date.unwrap_or_default()),
            ]),
            self.milestone_reply(owner, repo)?,
            "Milestone content update",
        )
        .await
    }
    pub async fn update_milestone_state(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        action: &str,
    ) -> Result<()> {
        if !matches!(action, "open" | "close") {
            return Err(GbMcpError::InvalidInput(
                "Invalid milestone state action".into(),
            ));
        }
        let url = self.repo_url(
            owner,
            repo,
            &["issues", "milestones", &number.to_string(), action],
        )?;
        self.write(
            self.client.get(url),
            self.milestone_reply(owner, repo)?,
            "Milestone state update",
        )
        .await
    }
    pub async fn find_label_id(&self, owner: &str, repo: &str, name: &str) -> Result<u64> {
        let url = self.repo_url(owner, repo, &["issues", "labels"])?;
        self.check_identity().await?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(GbMcpError::from)?;
        if response.status() != StatusCode::OK {
            return Err(GbMcpError::Authentication(
                "Could not read authenticated GitBucket label page",
            ));
        }
        let page_url = response.url().clone();
        let body =
            http::bounded_text(response, self.http_settings.html_limit_bytes, "HTML").await?;
        web_html::check_page(&body, &page_url, &self.api_base)?;
        web_html::label_id(&body, &self.repo_url(owner, repo, &["issues"])?, name)
    }
    pub async fn edit_label(
        &self,
        owner: &str,
        repo: &str,
        id: u64,
        name: &str,
        color: &str,
    ) -> Result<()> {
        let url = self.repo_url(owner, repo, &["issues", "labels", &id.to_string(), "edit"])?;
        let reply = Reply::Label {
            id,
            name: name.into(),
            issues: self.repo_url(owner, repo, &["issues"])?,
        };
        self.write(
            self.client.post(url).form(&[
                ("labelName", name.to_string()),
                ("labelColor", format!("#{color}")),
            ]),
            reply,
            "Label update",
        )
        .await
    }
}

impl std::fmt::Debug for GitBucketWebSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitBucketWebSession([REDACTED])")
    }
}
