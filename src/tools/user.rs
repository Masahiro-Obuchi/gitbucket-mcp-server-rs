use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;
use serde_json::{json, Map};
use std::borrow::Cow;

use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, validation_error, ToolResult};
use crate::tools::validation::required_trimmed;

#[derive(Debug, Default, Deserialize)]
pub struct GetAuthenticatedUserParams {}

impl JsonSchema for GetAuthenticatedUserParams {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("GetAuthenticatedUserParams")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        schema.insert("properties".to_string(), json!({}));
        schema.insert("required".to_string(), json!([]));
        schema.insert("additionalProperties".to_string(), json!(false));
        Schema::from(schema)
    }
}

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
    pub async fn get_user(&self, Parameters(params): Parameters<GetUserParams>) -> ToolResult {
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

    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{error_payload, success_json, MockApi, RecordedCall};
    use crate::tools::response::ToolErrorPayload;

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
