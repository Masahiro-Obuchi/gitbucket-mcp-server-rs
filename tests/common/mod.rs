use gitbucket_mcp_server::api::client::GitBucketClient;
use wiremock::MockServer;

pub struct TestServer {
    pub mock_server: MockServer,
    token: String,
}

impl TestServer {
    pub async fn start() -> Self {
        let mock_server = MockServer::start().await;
        Self {
            mock_server,
            token: "test-token".to_string(),
        }
    }

    pub fn client(&self) -> GitBucketClient {
        GitBucketClient::new(&self.mock_server.uri(), &self.token).unwrap()
    }

    #[allow(dead_code)]
    pub fn standard_client(&self) -> GitBucketClient {
        self.client()
            .with_api_profile(gitbucket_mcp_server::api::profile::ApiProfile::Standard)
    }

    #[allow(dead_code)]
    pub fn standard_client_with_web_auth(&self, username: &str, password: &str) -> GitBucketClient {
        GitBucketClient::new_with_web_auth(
            &self.mock_server.uri(),
            &self.token,
            false,
            Some(username),
            Some(password),
        )
        .unwrap()
        .with_api_profile(gitbucket_mcp_server::api::profile::ApiProfile::Standard)
    }
}

/// Realistic PAT vs cookie-only identity responses for standard web fixtures.
#[allow(dead_code)]
pub async fn web_identity(server: &MockServer, prefix: &str, login: &str) {
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    let login = login.to_string();
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/api/v3/user")))
        .respond_with(move |request: &wiremock::Request| {
            if request.headers.contains_key("authorization")
                || request.headers.contains_key("cookie")
            {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"login":login}))
            } else {
                ResponseTemplate::new(401)
            }
        })
        .mount(server)
        .await;
}

/// Synthetic REST merge reads for transport regressions; the write remains owned by each test.
#[allow(dead_code)]
pub async fn merge_reads(server: &MockServer, number: u64) {
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    let reads = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET")).and(path(format!("/api/v3/repos/owner/repo/pulls/{number}")))
        .respond_with(move |_:&wiremock::Request| {
            let after=reads.fetch_add(1,Ordering::SeqCst)>=2;
            ResponseTemplate::new(200).set_body_json(json!({"number":number,"title":"PR","state":if after{"closed"}else{"open"},"merged":after,"draft":false,"head":{"ref":"feature","sha":"b".repeat(40)},"base":{"ref":"main","sha":"a".repeat(40)}}))
        }).mount(server).await;
    let reads = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/branches/main"))
        .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"name":"main","commit":{"sha":if reads.fetch_add(1,Ordering::SeqCst)==0{"a".repeat(40)}else{"c".repeat(40)}}}))).mount(server).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/commits/{}",
            "c".repeat(40)
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"sha":"c".repeat(40),"parents":[{"sha":"a".repeat(40)},{"sha":"b".repeat(40)}]}),
        ))
        .mount(server)
        .await;
}
