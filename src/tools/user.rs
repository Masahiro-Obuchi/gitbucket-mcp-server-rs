use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, validation_error, ToolResult};
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
    ) -> ToolResult {
        match self.client.get_authenticated_user().await {
            Ok(user) => success(&user),
            Err(e) => from_gb_error(e),
        }
    }

    #[tool(description = "Get a GitBucket user by username")]
    pub async fn get_user(
        &self,
        Parameters(params): Parameters<GetUserParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let username = match required_trimmed(&params.username, "username") {
            Ok(username) => username,
            Err(err) => return validation_error(err),
        };

        match self.client.get_user(&username).await {
            Ok(user) => success(&user),
            Err(e) => from_gb_error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;

    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{MockApi, RecordedCall};
    use crate::tools::response::ToolErrorPayload;

    fn success_json(result: ToolResult) -> Value {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(false));
        result
            .structured_content
            .expect("expected structured content for success")
    }

    fn error_payload(result: ToolResult) -> ToolErrorPayload {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        serde_json::from_value(
            result
                .structured_content
                .expect("expected structured content for error"),
        )
        .expect("error payload should deserialize")
    }

    #[tokio::test]
    async fn test_get_user_rejects_blank_username() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .get_user(Parameters(GetUserParams {
                username: "  ".to_string(),
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "username must not be empty".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_get_user_uses_trimmed_username_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_user(Parameters(GetUserParams {
                username: "  alice  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["login"].as_str(), Some("mock-user"));
        match mock.calls().as_slice() {
            [RecordedCall::GetUser { username }] => assert_eq!(username, "alice"),
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_authenticated_user_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_authenticated_user(Parameters(GetAuthenticatedUserParams {}))
            .await;

        let result = success_json(result);
        assert_eq!(result["login"].as_str(), Some("mock-user"));
        match mock.calls().as_slice() {
            [RecordedCall::GetAuthenticatedUser] => {}
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
