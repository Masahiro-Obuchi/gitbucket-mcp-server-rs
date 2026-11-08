use rmcp::ServiceExt;
use std::{ffi::OsString, process::ExitCode};
use tracing_subscriber::EnvFilter;

use gitbucket_mcp_server::{
    api::{client::GitBucketClient, http::HttpSettings, profile::ApiProfile},
    config::Config,
    error::{GbMcpError, Result},
    server::{AccessMode, GitBucketMcpServer},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Serve,
    Help,
    Version,
    CheckConfig,
    CheckConnection,
}
impl Command {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self> {
        let Some(arg) = args.next() else {
            return Ok(Self::Serve);
        };
        if args.next().is_some() {
            return Err(GbMcpError::InvalidInput(
                "Choose one command; use --help".into(),
            ));
        }
        match arg.to_str() {
            Some("--help" | "-h") => Ok(Self::Help),
            Some("--version" | "-V") => Ok(Self::Version),
            Some("--check-config") => Ok(Self::CheckConfig),
            Some("--check-connection") => Ok(Self::CheckConnection),
            _ => Err(GbMcpError::InvalidInput(
                "Unknown command; use --help".into(),
            )),
        }
    }
}
const HELP: &str = "gitbucket-mcp-server - GitBucket MCP server over stdio

Usage: gitbucket-mcp-server [COMMAND]

Commands:
  --help, -h          Show help without reading configuration
  --version, -V       Show version without reading configuration
  --check-config      Validate local configuration without HTTP
  --check-connection  Validate configuration and read the PAT user's identity

With no command, waits for an MCP client on stdin/stdout. Ready is reported
on stderr only after the MCP initialize/initialized handshake completes.

Configuration: GITBUCKET_URL and GITBUCKET_TOKEN override config.toml.
If both are provided, the file is not opened. Web credentials are env-only:
GITBUCKET_USERNAME and GITBUCKET_PASSWORD must be set together.
No credential values or private instance URLs are printed by diagnostics.";

fn main() -> ExitCode {
    let command = match Command::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match command {
        Command::Help => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("gitbucket-mcp-server {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let filter = if command == Command::Serve {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    } else {
        // Diagnostics keep dependency debug output away from credential-bearing requests.
        EnvFilter::new("gitbucket_mcp_server=warn")
    };
    if tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .is_err()
    {
        eprintln!("Could not initialize logging");
        return ExitCode::FAILURE;
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("Could not initialize async runtime");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(command)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<()> {
    let config = Config::load()?;
    let settings = HttpSettings::from_env()?;
    let profile = ApiProfile::from_env()?;
    let access = AccessMode::from_env()?;
    let client = GitBucketClient::new_with_web_auth_and_http_settings(
        &config.gitbucket_url,
        &config.gitbucket_token,
        false,
        config.gitbucket_username.as_deref(),
        config.gitbucket_password.as_deref(),
        settings,
    )?
    .with_api_profile(profile);
    if matches!(command, Command::CheckConfig | Command::CheckConnection) {
        let checked = command == Command::CheckConnection;
        if checked {
            let user = client.get_authenticated_user().await?;
            if user.login.trim().is_empty() {
                return Err(GbMcpError::Authentication(
                    "Connection check did not identify a user",
                ));
            }
            if config
                .gitbucket_username
                .as_deref()
                .is_some_and(|username| username != user.login)
            {
                return Err(GbMcpError::Authentication("PAT identity differs from configured Web username; Web password was not submitted"));
            }
        }
        println!(
            "{}",
            serde_json::json!({
                "status":"ok","mode":if checked{"connection"}else{"configuration"},
                "connection_checked":checked,"web_session_checked":false,
                "web_credentials_configured":config.gitbucket_username.is_some(),
                "api_profile": match profile {
                    ApiProfile::RestOnly => "rest-only",
                    ApiProfile::Standard => "standard",
                },
                "access_mode": if access == AccessMode::ReadOnly {"read_only"} else {"read_write"},
            })
        );
        return Ok(());
    }
    eprintln!("gitbucket-mcp-server starting over stdio; waiting for MCP initialization");
    let service = GitBucketMcpServer::new(client)
        .with_access_mode(access)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|_| GbMcpError::Other("MCP initialization failed".into()))?;
    eprintln!("gitbucket-mcp-server ready (MCP initialized)");
    service
        .waiting()
        .await
        .map_err(|_| GbMcpError::Other("MCP server task failed".into()))?;
    Ok(())
}
