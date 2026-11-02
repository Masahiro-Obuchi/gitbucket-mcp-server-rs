use std::sync::Arc;

use crate::{api::http, error::GbMcpError, tools::response::from_gb_error};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::ServerHandler;
use std::time::Duration;

use crate::api::{client::GitBucketClient, GitBucketApi};

#[derive(Debug, Clone)]
pub struct GitBucketMcpServer {
    pub(crate) client: Arc<dyn GitBucketApi>,
    tool_router: ToolRouter<Self>,
    tool_timeout: Duration,
}

impl GitBucketMcpServer {
    pub fn new(client: GitBucketClient) -> Self {
        let tool_timeout = client.http_settings().tool_timeout;
        Self::build(Arc::new(client), tool_timeout)
    }

    pub fn with_api(client: Arc<dyn GitBucketApi>) -> Self {
        Self::build(client, http::HttpSettings::default().tool_timeout)
    }

    pub fn with_api_and_timeout(
        client: Arc<dyn GitBucketApi>,
        tool_timeout: Duration,
    ) -> crate::error::Result<Self> {
        http::HttpSettings {
            tool_timeout,
            ..Default::default()
        }
        .validate()?;
        Ok(Self::build(client, tool_timeout))
    }

    fn build(client: Arc<dyn GitBucketApi>, tool_timeout: Duration) -> Self {
        let tool_router = Self::tool_router_user()
            + Self::tool_router_repository()
            + Self::tool_router_label()
            + Self::tool_router_milestone()
            + Self::tool_router_issue()
            + Self::tool_router_pull_request();
        Self {
            client,
            tool_router,
            tool_timeout,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_api(client: Arc<dyn GitBucketApi>) -> Self {
        Self::with_api(client)
    }
}

impl ServerHandler for GitBucketMcpServer {
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let cancellation = context.ct.clone();
        // Unknown adapters cannot prove a mutation has not started. These
        // built-in read tool names are independent of any transport tracking.
        let read_only = matches!(
            request.name.as_ref(),
            "get_authenticated_user"
                | "get_user"
                | "list_repositories"
                | "get_repository"
                | "list_branches"
                | "list_labels"
                | "get_label"
                | "list_milestones"
                | "get_milestone"
                | "list_issues"
                | "get_issue"
                | "list_issue_comments"
                | "list_pull_requests"
                | "get_pull_request"
        );
        let untracked_write = !self.client.tracks_write_attempts() && !read_only;
        let tool_context =
            rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        http::WRITE_POSSIBLE.scope(Arc::new(http::WriteTracker::new(untracked_write)), async {
            let error = tokio::select! {
                biased;
                _ = cancellation.cancelled() => GbMcpError::Cancelled,
                result = tokio::time::timeout(self.tool_timeout, self.tool_router.call(tool_context)) => {
                    match result { Ok(result) => return result, Err(_) => GbMcpError::Timeout { phase: "MCP tool execution" } }
                }
            };
            // The dropped future cannot undo a remote mutation. Never replay it.
            from_gb_error(if http::write_possible() { http::unknown("Tool execution", error) } else { error })
        }).await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.tool_router.get(name).cloned()
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "GitBucket MCP Server - Interact with GitBucket repositories, issues, and pull requests.",
            )
            .with_server_info(Implementation::new(
                "gitbucket-mcp-server",
                env!("CARGO_PKG_VERSION"),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_info() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);
        let info = server.get_info();

        assert!(info.instructions.is_some());
        assert!(info.instructions.unwrap().contains("GitBucket MCP Server"));
    }
    #[tokio::test]
    async fn custom_api_without_tracking_keeps_write_deadlines_conservative() {
        use rmcp::{model::CallToolRequestParams, ServiceExt};
        let mut mock = crate::test_support::MockApi::default();
        mock.response_delay = Duration::from_secs(1);
        let api = Arc::new(mock);
        assert!(GitBucketMcpServer::with_api_and_timeout(api.clone(), Duration::ZERO).is_err());
        let server =
            GitBucketMcpServer::with_api_and_timeout(api, Duration::from_millis(100)).unwrap();
        let (server_transport, client_transport) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let running = server.serve(server_transport).await.unwrap();
            let _ = running.waiting().await;
        });
        let client = ().serve(client_transport).await.unwrap();
        let result = client
            .call_tool(
                CallToolRequestParams::new("create_issue").with_arguments(
                    serde_json::json!({"owner":"owner","repo":"repo","title":"test"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            result.structured_content.unwrap()["kind"],
            "outcome_unknown"
        );
        let result = client
            .call_tool(CallToolRequestParams::new("get_authenticated_user"))
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["kind"], "timeout_error");
        client.cancel().await.unwrap();
    }
}
