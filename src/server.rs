use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool_handler, ServerHandler};

use crate::api::{client::GitBucketClient, GitBucketApi};

#[derive(Debug, Clone)]
pub struct GitBucketMcpServer {
    pub(crate) client: Arc<dyn GitBucketApi>,
    tool_router: ToolRouter<Self>,
}

impl GitBucketMcpServer {
    pub fn new(client: GitBucketClient) -> Self {
        Self::with_api(Arc::new(client))
    }

    pub fn with_api(client: Arc<dyn GitBucketApi>) -> Self {
        let tool_router = Self::tool_router_user()
            + Self::tool_router_repository()
            + Self::tool_router_issue()
            + Self::tool_router_pull_request();
        Self {
            client,
            tool_router,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_api(client: Arc<dyn GitBucketApi>) -> Self {
        Self::with_api(client)
    }
}

#[tool_handler]
impl ServerHandler for GitBucketMcpServer {
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
}
