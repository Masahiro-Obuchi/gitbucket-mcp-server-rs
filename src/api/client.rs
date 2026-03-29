use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::{ApiFuture, GitBucketApi};
use crate::error::{GbMcpError, Result};
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};
use crate::models::pull_request::{CreatePullRequest, MergePullRequest, MergeResult, PullRequest};
use crate::models::repository::{Branch, CreateRepository, Repository};
use crate::models::user::User;

#[derive(Debug, Clone)]
pub struct GitBucketClient {
    client: reqwest::Client,
    base_url: String,
}

impl GitBucketClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        Self::new_with_options(base_url, token, false)
    }

    pub fn new_with_options(
        base_url: &str,
        token: &str,
        allow_invalid_certs: bool,
    ) -> Result<Self> {
        let normalized = normalize_base_url(base_url)?;

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
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(GbMcpError::Http)?;
        self.handle_response(resp).await
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
        self.handle_response(resp).await
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
        self.handle_response(resp).await
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
        self.handle_response(resp).await
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
            Err(GbMcpError::Api { status, message })
        }
    }

    async fn handle_response<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await.map_err(GbMcpError::Http)?;
            parse_success_body(&text)
        } else {
            let status_code = status.as_u16();
            let message = resp.text().await.unwrap_or_default();
            Err(GbMcpError::Api {
                status: status_code,
                message,
            })
        }
    }
}

fn parse_success_body<T: DeserializeOwned>(body: &str) -> Result<T> {
    let value: Value = serde_json::from_str(body).map_err(GbMcpError::Json)?;

    if let Some(inner) = value.as_str() {
        return serde_json::from_str(inner).map_err(GbMcpError::Json);
    }

    if value.get("status").is_some() {
        if let Some(inner) = value.get("body") {
            return match inner {
                Value::String(text) => serde_json::from_str(text).map_err(GbMcpError::Json),
                other => serde_json::from_value(other.clone()).map_err(GbMcpError::Json),
            };
        }
    }

    serde_json::from_value(value).map_err(GbMcpError::Json)
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
}
