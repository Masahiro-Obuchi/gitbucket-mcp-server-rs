use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, warn};
use url::Url;

use super::{ApiFuture, GitBucketApi};
use crate::error::{GbMcpError, Result};
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};
use crate::models::label::{CreateLabel, Label, UpdateLabel};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};
use crate::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest, UpdatePullRequest,
};
use crate::models::repository::{Branch, CreateRepository, Repository};
use crate::models::user::User;

#[derive(Debug, Clone)]
pub struct GitBucketClient {
    client: reqwest::Client,
    base_url: String,
    allow_invalid_certs: bool,
    web_credentials: Option<WebCredentials>,
}

#[derive(Debug, Clone)]
pub(crate) struct WebCredentials {
    pub username: String,
    pub password: String,
}

impl GitBucketClient {
    const DEFAULT_PER_PAGE: usize = 100;
    const MAX_PAGINATION_PAGES: usize = 100;

    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Self::new_with_options(base_url, token, false)
    }

    pub fn new_with_options(
        base_url: &str,
        token: &str,
        allow_invalid_certs: bool,
    ) -> Result<Self> {
        Self::new_with_web_auth(base_url, token, allow_invalid_certs, None, None)
    }

    pub fn new_with_web_auth(
        base_url: &str,
        token: &str,
        allow_invalid_certs: bool,
        web_username: Option<&str>,
        web_password: Option<&str>,
    ) -> Result<Self> {
        let normalized = normalize_base_url(base_url)?;
        let web_credentials = resolve_web_credentials(web_username, web_password)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("token {}", token))
                .map_err(|e| GbMcpError::Other(format!("Invalid token header: {}", e)))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(allow_invalid_certs)
            .build()
            .map_err(GbMcpError::Http)?;

        Ok(Self {
            client,
            base_url: normalized,
            allow_invalid_certs,
            web_credentials,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn allow_invalid_certs(&self) -> bool {
        self.allow_invalid_certs
    }

    pub(crate) fn web_credentials(&self) -> Option<&WebCredentials> {
        self.web_credentials.as_ref()
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "GET", path).await
    }

    pub(crate) async fn get_url<T: DeserializeOwned>(&self, url: Url, path: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "GET", path).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "POST", path).await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .patch(&url)
            .json(body)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "PATCH", path).await
    }

    pub(crate) async fn patch_url<T: DeserializeOwned, B: Serialize>(
        &self,
        url: Url,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .client
            .patch(url)
            .json(body)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "PATCH", path).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .put(&url)
            .json(body)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp, "PUT", path).await
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(GbMcpError::Http)?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            warn!(
                method = "DELETE",
                path, status, "GitBucket API request failed"
            );
            Err(GbMcpError::Api { status, message })
        }
    }

    pub(crate) async fn delete_url(&self, url: Url, path: &str) -> Result<()> {
        let resp = self
            .client
            .delete(url)
            .send()
            .await
            .map_err(GbMcpError::Http)?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            warn!(
                method = "DELETE",
                path, status, "GitBucket API request failed"
            );
            Err(GbMcpError::Api { status, message })
        }
    }

    pub async fn get_paginated<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<T>> {
        let mut results = Vec::new();

        for page in 1..=Self::MAX_PAGINATION_PAGES {
            let url = self.paginated_url(path, query, page)?;
            let resp = self
                .client
                .get(url)
                .send()
                .await
                .map_err(GbMcpError::Http)?;
            let items: Vec<T> = self.handle_response(resp, "GET", path).await?;
            let item_count = items.len();
            results.extend(items);

            if item_count < Self::DEFAULT_PER_PAGE {
                return Ok(results);
            }
        }

        Err(GbMcpError::Other(format!(
            "Pagination exceeded {} pages for {}",
            Self::MAX_PAGINATION_PAGES,
            path
        )))
    }

    async fn handle_response<T: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
        method: &str,
        path: &str,
    ) -> Result<T> {
        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await.map_err(GbMcpError::Http)?;
            debug!(
                method,
                path,
                status = status.as_u16(),
                "GitBucket API request succeeded"
            );
            parse_success_body(&text)
        } else {
            let status_code = status.as_u16();
            let message = resp.text().await.unwrap_or_default();
            warn!(
                method,
                path,
                status = status_code,
                "GitBucket API request failed"
            );
            Err(GbMcpError::Api {
                status: status_code,
                message,
            })
        }
    }

    fn paginated_url(&self, path: &str, query: &[(&str, &str)], page: usize) -> Result<Url> {
        let mut url =
            Url::parse(&format!("{}{}", self.base_url, path)).map_err(GbMcpError::UrlParse)?;
        {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
            pairs.append_pair("per_page", &Self::DEFAULT_PER_PAGE.to_string());
            pairs.append_pair("page", &page.to_string());
        }
        Ok(url)
    }
}

fn parse_success_body<T: DeserializeOwned>(body: &str) -> Result<T> {
    if body.trim().is_empty() {
        return Err(GbMcpError::Other(
            "GitBucket API returned an empty response body".to_string(),
        ));
    }

    let value: Value = serde_json::from_str(body).map_err(GbMcpError::Json)?;

    if let Some((status, message)) = wrapped_error(&value) {
        return Err(GbMcpError::Api { status, message });
    }

    if let Some(inner) = value.as_str() {
        if inner.trim().is_empty() {
            return Err(GbMcpError::Other(
                "GitBucket API returned an empty response body".to_string(),
            ));
        }
        return serde_json::from_str(inner).map_err(GbMcpError::Json);
    }

    if value.get("status").is_some() {
        if let Some(inner) = value.get("body") {
            return match inner {
                Value::String(text) if text.trim().is_empty() => Err(GbMcpError::Other(
                    "GitBucket API returned an empty response body".to_string(),
                )),
                Value::String(text) => serde_json::from_str(text).map_err(GbMcpError::Json),
                other => serde_json::from_value(other.clone()).map_err(GbMcpError::Json),
            };
        }
    }

    serde_json::from_value(value).map_err(GbMcpError::Json)
}

fn wrapped_error(value: &Value) -> Option<(u16, String)> {
    let status = value.get("status").and_then(status_code_from_value)?;
    if (200..300).contains(&status) {
        return None;
    }

    Some((status, wrapped_error_message(value, status)))
}

fn status_code_from_value(value: &Value) -> Option<u16> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|status| u16::try_from(status).ok()),
        Value::String(text) => text.parse::<u16>().ok(),
        _ => None,
    }
}

fn wrapped_error_message(value: &Value, status: u16) -> String {
    value
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("body")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("body")
                .filter(|body| {
                    !body.is_null()
                        && !body
                            .as_str()
                            .is_some_and(|message| message.trim().is_empty())
                })
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| {
            reqwest::StatusCode::from_u16(status)
                .ok()
                .and_then(|status| status.canonical_reason().map(str::to_string))
                .unwrap_or_else(|| "GitBucket API request failed".to_string())
        })
}

fn resolve_web_credentials(
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Option<WebCredentials>> {
    match (username, password) {
        (None, None) => Ok(None),
        (Some(username), Some(password)) => {
            if username.trim().is_empty() {
                return Err(GbMcpError::Config(
                    "GITBUCKET_USERNAME must not be empty".to_string(),
                ));
            }
            if password.trim().is_empty() {
                return Err(GbMcpError::Config(
                    "GITBUCKET_PASSWORD must not be empty".to_string(),
                ));
            }

            Ok(Some(WebCredentials {
                username: username.to_string(),
                password: password.to_string(),
            }))
        }
        (Some(_), None) | (None, Some(_)) => Err(GbMcpError::Config(
            "GITBUCKET_USERNAME and GITBUCKET_PASSWORD must be set together".to_string(),
        )),
    }
}

impl GitBucketApi for GitBucketClient {
    fn get_authenticated_user(&self) -> ApiFuture<'_, User> {
        Box::pin(async move { GitBucketClient::get_authenticated_user(self).await })
    }

    fn get_user<'a>(&'a self, username: &'a str) -> ApiFuture<'a, User> {
        Box::pin(async move { GitBucketClient::get_user(self, username).await })
    }

    fn list_repositories<'a>(&'a self, owner: &'a str) -> ApiFuture<'a, Vec<Repository>> {
        Box::pin(async move { GitBucketClient::list_repositories(self, owner).await })
    }

    fn get_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::get_repository(self, owner, repo).await })
    }

    fn create_repository<'a>(&'a self, body: &'a CreateRepository) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::create_repository(self, body).await })
    }

    fn fork_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::fork_repository(self, owner, repo).await })
    }

    fn list_branches<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Branch>> {
        Box::pin(async move { GitBucketClient::list_branches(self, owner, repo).await })
    }

    fn list_labels<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Label>> {
        Box::pin(async move { GitBucketClient::list_labels(self, owner, repo).await })
    }

    fn get_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::get_label(self, owner, repo, name).await })
    }

    fn create_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateLabel,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::create_label(self, owner, repo, body).await })
    }

    fn update_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
        body: &'a UpdateLabel,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::update_label(self, owner, repo, name, body).await })
    }

    fn delete_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move { GitBucketClient::delete_label(self, owner, repo, name).await })
    }

    fn list_milestones<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Milestone>> {
        Box::pin(async move { GitBucketClient::list_milestones(self, owner, repo, state).await })
    }

    fn get_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(async move { GitBucketClient::get_milestone(self, owner, repo, number).await })
    }

    fn create_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(async move { GitBucketClient::create_milestone(self, owner, repo, body).await })
    }

    fn update_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(
            async move { GitBucketClient::update_milestone(self, owner, repo, number, body).await },
        )
    }

    fn delete_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move { GitBucketClient::delete_milestone(self, owner, repo, number).await })
    }

    fn list_issues<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Issue>> {
        Box::pin(async move { GitBucketClient::list_issues(self, owner, repo, state).await })
    }

    fn get_issue<'a>(&'a self, owner: &'a str, repo: &'a str, number: u64) -> ApiFuture<'a, Issue> {
        Box::pin(async move { GitBucketClient::get_issue(self, owner, repo, number).await })
    }

    fn create_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateIssue,
    ) -> ApiFuture<'a, Issue> {
        Box::pin(async move { GitBucketClient::create_issue(self, owner, repo, body).await })
    }

    fn update_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateIssue,
    ) -> ApiFuture<'a, Issue> {
        Box::pin(
            async move { GitBucketClient::update_issue(self, owner, repo, number, body).await },
        )
    }

    fn list_issue_comments<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Vec<Comment>> {
        Box::pin(
            async move { GitBucketClient::list_issue_comments(self, owner, repo, number).await },
        )
    }

    fn add_issue_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        Box::pin(async move {
            GitBucketClient::add_issue_comment(self, owner, repo, number, body).await
        })
    }

    fn list_pull_requests<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<PullRequest>> {
        Box::pin(async move { GitBucketClient::list_pull_requests(self, owner, repo, state).await })
    }

    fn get_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move { GitBucketClient::get_pull_request(self, owner, repo, number).await })
    }

    fn create_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move { GitBucketClient::create_pull_request(self, owner, repo, body).await })
    }

    fn update_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move {
            GitBucketClient::update_pull_request(self, owner, repo, number, body).await
        })
    }

    fn merge_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a MergePullRequest,
    ) -> ApiFuture<'a, MergeResult> {
        Box::pin(async move {
            GitBucketClient::merge_pull_request(self, owner, repo, number, body).await
        })
    }

    fn add_pull_request_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        Box::pin(async move {
            GitBucketClient::add_pull_request_comment(self, owner, repo, number, body).await
        })
    }
}

/// Normalize a base URL for the GitBucket API.
///
/// Ensures the URL has a scheme, strips trailing slashes, and appends `/api/v3` if not present.
pub fn normalize_base_url(input: &str) -> Result<String> {
    let trimmed = input.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        return Err(GbMcpError::Config("Base URL must not be empty".to_string()));
    }

    // Add https:// scheme if missing
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    };

    // Parse to validate
    let parsed = url::Url::parse(&with_scheme).map_err(GbMcpError::UrlParse)?;

    // Reconstruct: scheme + host + optional port + path
    let mut base = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or("localhost")
    );
    if let Some(port) = parsed.port() {
        base = format!("{}:{}", base, port);
    }

    let path = parsed.path().trim_end_matches('/');
    if !path.is_empty() {
        base.push_str(path);
    }

    // Append /api/v3 if not already present
    if !base.ends_with("/api/v3") {
        base.push_str("/api/v3");
    }

    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct WrappedValue {
        name: String,
    }

    #[test]
    fn test_normalize_simple_hostname() {
        let result = normalize_base_url("gitbucket.example.com").unwrap();
        assert_eq!(result, "https://gitbucket.example.com/api/v3");
    }

    #[test]
    fn test_normalize_with_https() {
        let result = normalize_base_url("https://gitbucket.example.com").unwrap();
        assert_eq!(result, "https://gitbucket.example.com/api/v3");
    }

    #[test]
    fn test_normalize_with_http() {
        let result = normalize_base_url("http://localhost:8080").unwrap();
        assert_eq!(result, "http://localhost:8080/api/v3");
    }

    #[test]
    fn test_normalize_with_path_prefix() {
        let result = normalize_base_url("https://example.com/gitbucket").unwrap();
        assert_eq!(result, "https://example.com/gitbucket/api/v3");
    }

    #[test]
    fn test_normalize_already_has_api_v3() {
        let result = normalize_base_url("https://gitbucket.example.com/api/v3").unwrap();
        assert_eq!(result, "https://gitbucket.example.com/api/v3");
    }

    #[test]
    fn test_normalize_trailing_slash() {
        let result = normalize_base_url("https://gitbucket.example.com/").unwrap();
        assert_eq!(result, "https://gitbucket.example.com/api/v3");
    }

    #[test]
    fn test_normalize_empty_fails() {
        let result = normalize_base_url("");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_whitespace_only_fails() {
        let result = normalize_base_url("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_new_client_creates_successfully() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token");
        assert!(client.is_ok());
        assert_eq!(
            client.unwrap().base_url(),
            "https://gitbucket.example.com/api/v3"
        );
    }

    #[test]
    fn test_new_client_with_web_auth_creates_successfully() {
        let client = GitBucketClient::new_with_web_auth(
            "https://gitbucket.example.com",
            "test-token",
            false,
            Some("alice"),
            Some("secret-pass"),
        )
        .unwrap();

        assert_eq!(client.web_credentials().unwrap().username, "alice");
    }

    #[test]
    fn test_new_client_with_partial_web_auth_fails() {
        let err = GitBucketClient::new_with_web_auth(
            "https://gitbucket.example.com",
            "test-token",
            false,
            Some("alice"),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("must be set together"));
    }

    #[test]
    fn test_parse_success_body_accepts_wrapped_json_string() {
        let value: WrappedValue = parse_success_body(r#""{\"name\":\"wrapped\"}""#).unwrap();
        assert_eq!(
            value,
            WrappedValue {
                name: "wrapped".to_string()
            }
        );
    }

    #[test]
    fn test_parse_success_body_accepts_status_body_wrapper() {
        let value: WrappedValue =
            parse_success_body(r#"{"status":"ok","body":"{\"name\":\"wrapped\"}"}"#).unwrap();
        assert_eq!(
            value,
            WrappedValue {
                name: "wrapped".to_string()
            }
        );
    }

    #[test]
    fn test_parse_success_body_maps_wrapped_404_to_api_error() {
        let err = parse_success_body::<WrappedValue>(r#"{"status":404,"body":""}"#).unwrap_err();

        match err {
            GbMcpError::Api { status, message } => {
                assert_eq!(status, 404);
                assert_eq!(message, "Not Found");
            }
            err => panic!("expected wrapped API error, got {err:?}"),
        }
    }

    #[test]
    fn test_parse_success_body_maps_wrapped_error_message_to_api_error() {
        let err = parse_success_body::<WrappedValue>(
            r#"{"status":"422","message":"Validation Failed","body":""}"#,
        )
        .unwrap_err();

        match err {
            GbMcpError::Api { status, message } => {
                assert_eq!(status, 422);
                assert_eq!(message, "Validation Failed");
            }
            err => panic!("expected wrapped API error, got {err:?}"),
        }
    }
}
