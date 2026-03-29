use gitbucket_mcp_server::api::client::GitBucketClient;
use gitbucket_mcp_server::server::GitBucketMcpServer;
use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt;
use serde_json::{json, Value};
use serial_test::serial;

struct E2eConfig {
    url: String,
    token: String,
    owner: Option<String>,
    repo: Option<String>,
    allow_invalid_certs: bool,
}

impl E2eConfig {
    fn from_env() -> Self {
        Self {
            url: required_env("GITBUCKET_E2E_URL"),
            token: required_env("GITBUCKET_E2E_TOKEN"),
            owner: optional_env("GITBUCKET_E2E_OWNER"),
            repo: optional_env("GITBUCKET_E2E_REPO"),
            allow_invalid_certs: optional_env("GITBUCKET_E2E_INSECURE_TLS")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false),
        }
    }

    fn repository_target(&self) -> (&str, &str) {
        let owner = self
            .owner
            .as_deref()
            .expect("GITBUCKET_E2E_OWNER is required for repository-scoped E2E tests");
        let repo = self
            .repo
            .as_deref()
            .expect("GITBUCKET_E2E_REPO is required for repository-scoped E2E tests");
        (owner, repo)
    }
}

async fn spawn_client_and_server(
    config: &E2eConfig,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let client =
        GitBucketClient::new_with_options(&config.url, &config.token, config.allow_invalid_certs)
            .unwrap();

    tokio::spawn(async move {
        let server = GitBucketMcpServer::new(client)
            .serve(server_transport)
            .await
            .unwrap();
        server.waiting().await.unwrap();
    });

    ().serve(client_transport).await.unwrap()
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set to run E2E tests"))
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_text(result: &rmcp::model::CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("expected text tool result")
}

fn parse_json(result: &rmcp::model::CallToolResult) -> Value {
    let text = first_text(result);
    assert!(
        !text.starts_with("Error:"),
        "expected JSON response, got error: {text}"
    );
    serde_json::from_str(text).expect("tool result should be valid JSON")
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL and GITBUCKET_E2E_TOKEN"]
#[serial]
async fn test_e2e_get_authenticated_user() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_authenticated_user")
                .with_arguments(json!({}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();

    let user = parse_json(&result);
    let login = user["login"]
        .as_str()
        .expect("authenticated user should have login");
    assert!(!login.is_empty());

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL and GITBUCKET_E2E_TOKEN"]
#[serial]
async fn test_e2e_get_user_for_authenticated_login() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;

    let authenticated = client
        .call_tool(
            CallToolRequestParams::new("get_authenticated_user")
                .with_arguments(json!({}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let login = parse_json(&authenticated)["login"]
        .as_str()
        .expect("authenticated user should have login")
        .to_string();

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_user").with_arguments(
                json!({
                    "username": login
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    let user = parse_json(&result);
    assert_eq!(
        user["login"].as_str(),
        authenticated_user_login(&authenticated).as_deref()
    );

    client.cancel().await.unwrap();
}

fn authenticated_user_login(result: &rmcp::model::CallToolResult) -> Option<String> {
    parse_json(result)["login"].as_str().map(str::to_string)
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL and GITBUCKET_E2E_TOKEN"]
#[serial]
async fn test_e2e_list_repositories_for_owner() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;

    let owner = match config.owner.as_deref() {
        Some(owner) => owner.to_string(),
        None => {
            let authenticated = client
                .call_tool(
                    CallToolRequestParams::new("get_authenticated_user")
                        .with_arguments(json!({}).as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            authenticated_user_login(&authenticated).expect("authenticated user should have login")
        }
    };

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_repositories").with_arguments(
                json!({
                    "owner": owner
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(parse_json(&result).is_array());

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_get_repository() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_repository").with_arguments(
                json!({
                    "owner": owner,
                    "repo": repo
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    let repository = parse_json(&result);
    let expected_full_name = format!("{owner}/{repo}");
    assert_eq!(repository["name"].as_str(), Some(repo));
    assert_eq!(
        repository["full_name"].as_str(),
        Some(expected_full_name.as_str())
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_list_issues() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_issues").with_arguments(
                json!({
                    "owner": owner,
                    "repo": repo,
                    "state": "all"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(parse_json(&result).is_array());

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_list_pull_requests() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_pull_requests").with_arguments(
                json!({
                    "owner": owner,
                    "repo": repo,
                    "state": "all"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(parse_json(&result).is_array());

    client.cancel().await.unwrap();
}
