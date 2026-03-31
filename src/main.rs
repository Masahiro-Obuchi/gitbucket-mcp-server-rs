use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use gitbucket_mcp_server::api::client::GitBucketClient;
use gitbucket_mcp_server::config::Config;
use gitbucket_mcp_server::server::GitBucketMcpServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to stderr (stdout is used for MCP protocol)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Load configuration (TOML file + environment variables)
    let config = Config::load().map_err(|e| {
        eprintln!("Configuration error: {}", e);
        e
    })?;
    eprintln!(
        "gitbucket-mcp-server starting for {} over stdio",
        config.gitbucket_url
    );
    tracing::info!("Starting GitBucket MCP Server");

    // Create API client
    let client = GitBucketClient::new_with_web_auth(
        &config.gitbucket_url,
        &config.gitbucket_token,
        false,
        config.gitbucket_username.as_deref(),
        config.gitbucket_password.as_deref(),
    )?;

    // Create and start MCP server via stdio transport
    let server = GitBucketMcpServer::new(client);
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("Failed to start MCP server: {}", e);
        })?;

    tracing::info!("GitBucket MCP Server is running");
    eprintln!("gitbucket-mcp-server ready");
    service.waiting().await?;

    Ok(())
}
