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
