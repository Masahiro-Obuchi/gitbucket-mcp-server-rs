use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::GitBucketMcpServer;

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
        match self.client.get_user(&params.username).await {
            Ok(user) => serde_json::to_string_pretty(&user)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}
