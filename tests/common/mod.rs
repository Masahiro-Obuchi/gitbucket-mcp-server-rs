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

    pub fn client_with_web_auth(&self, username: &str, password: &str) -> GitBucketClient {
        GitBucketClient::new_with_web_auth(
            &self.mock_server.uri(),
            &self.token,
            false,
            Some(username),
            Some(password),
        )
        .unwrap()
    }
}
