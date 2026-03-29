use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::GitBucketMcpServer;
use crate::tools::validation::required_trimmed;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetAuthenticatedUserParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetUserParams {
    #[schemars(description = "The username to look up")]
    pub username: String,
}

#[tool_router(router = tool_router_user, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "Get the currently authenticated GitBucket user")]
    pub async fn get_authenticated_user(
        &self,
        Parameters(_params): Parameters<GetAuthenticatedUserParams>,
    ) -> String {
        match self.client.get_authenticated_user().await {
            Ok(user) => serde_json::to_string_pretty(&user)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Get a GitBucket user by username")]
    pub async fn get_user(&self, Parameters(params): Parameters<GetUserParams>) -> String {
        let username = match required_trimmed(&params.username, "username") {
            Ok(username) => username,
            Err(err) => return err,
        };

        match self.client.get_user(&username).await {
            Ok(user) => serde_json::to_string_pretty(&user)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    use crate::api::client::GitBucketClient;

    #[tokio::test]
    async fn test_get_user_rejects_blank_username() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .get_user(Parameters(GetUserParams {
                username: "  ".to_string(),
            }))
            .await;

        assert_eq!(result, "Error: username must not be empty");
    }
}
