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
    pub fn legacy_client(&self) -> GitBucketClient {
        self.client()
            .with_compatibility(gitbucket_mcp_server::api::compatibility::Compatibility::Legacy404)
    }

    #[allow(dead_code)]
    pub fn legacy_client_with_web_auth(&self, username: &str, password: &str) -> GitBucketClient {
        GitBucketClient::new_with_web_auth(
            &self.mock_server.uri(),
            &self.token,
            false,
            Some(username),
            Some(password),
        )
        .unwrap()
        .with_compatibility(gitbucket_mcp_server::api::compatibility::Compatibility::Legacy404)
    }
}
