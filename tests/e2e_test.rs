use gitbucket_mcp_server::api::client::GitBucketClient;
use gitbucket_mcp_server::server::GitBucketMcpServer;
use gitbucket_mcp_server::tools::response::ToolErrorPayload;
use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt;
use serde_json::{json, Value};
use serial_test::serial;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use url::Url;

struct E2eConfig {
    url: String,
    token: String,
    owner: Option<String>,
    repo: Option<String>,
    git_username: Option<String>,
    git_password: Option<String>,
    web_username: Option<String>,
    web_password: Option<String>,
    allow_invalid_certs: bool,
}

impl E2eConfig {
    fn from_env() -> Self {
        Self {
            url: required_env("GITBUCKET_E2E_URL"),
            token: required_env("GITBUCKET_E2E_TOKEN"),
            owner: optional_env("GITBUCKET_E2E_OWNER"),
            repo: optional_env("GITBUCKET_E2E_REPO"),
            git_username: optional_env("GITBUCKET_E2E_GIT_USERNAME"),
            git_password: optional_env("GITBUCKET_E2E_GIT_PASSWORD"),
            web_username: optional_env("GITBUCKET_E2E_WEB_USERNAME")
                .or_else(|| optional_env("GITBUCKET_E2E_GIT_USERNAME")),
            web_password: optional_env("GITBUCKET_E2E_WEB_PASSWORD")
                .or_else(|| optional_env("GITBUCKET_E2E_GIT_PASSWORD")),
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

    fn git_credentials(&self) -> (&str, &str) {
        let username = self
            .git_username
            .as_deref()
            .expect("GITBUCKET_E2E_GIT_USERNAME is required for PR write-path E2E tests");
        let password = self
            .git_password
            .as_deref()
            .expect("GITBUCKET_E2E_GIT_PASSWORD is required for PR write-path E2E tests");

        (username, password)
    }
}

async fn spawn_client_and_server(
    config: &E2eConfig,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let client = GitBucketClient::new_with_web_auth(
        &config.url,
        &config.token,
        config.allow_invalid_certs,
        config.web_username.as_deref(),
        config.web_password.as_deref(),
    )
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
    assert_eq!(result.is_error, Some(false), "expected success tool result");
    result.structured_content.clone().unwrap_or_else(|| {
        serde_json::from_str(first_text(result)).expect("tool result should be valid JSON")
    })
}

fn parse_error(result: &rmcp::model::CallToolResult) -> ToolErrorPayload {
    assert_eq!(result.is_error, Some(true), "expected error tool result");
    serde_json::from_value(result.structured_content.clone().unwrap_or_else(|| {
        serde_json::from_str(first_text(result)).expect("tool result should be valid JSON")
    }))
    .expect("tool error should deserialize")
}

async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &'static str,
    arguments: Value,
) -> rmcp::model::CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}

async fn call_tool_json(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &'static str,
    arguments: Value,
) -> Value {
    let result = call_tool(client, name, arguments).await;
    parse_json(&result)
}

fn unique_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    format!("{}-{}", std::process::id(), now)
}

fn git_repo_url(config: &E2eConfig, owner: &str, repo: &str) -> String {
    let (username, password) = config.git_credentials();
    let mut url = Url::parse(&config.url).expect("GITBUCKET_E2E_URL must be a valid URL");
    let mut path = url.path().trim_end_matches('/').to_string();
    path.push_str(&format!("/{owner}/{repo}.git"));
    url.set_path(&path);
    url.set_username(username)
        .expect("git username should be URL-safe");
    url.set_password(Some(password))
        .expect("git password should be URL-safe");
    url.to_string()
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command should start");

    assert!(
        output.status.success(),
        "git command failed in {}:\nstdout:\n{}\nstderr:\n{}",
        dir.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn repository_default_branch(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    owner: &str,
    repo: &str,
) -> String {
    let repository = call_tool_json(
        client,
        "get_repository",
        json!({
            "owner": owner,
            "repo": repo,
        }),
    )
    .await;

    repository["default_branch"]
        .as_str()
        .filter(|branch| !branch.is_empty())
        .expect("repository should expose a default branch for PR E2E")
        .to_string()
}

#[derive(Debug)]
struct PrBranchSeed {
    base_branch: String,
    feature_branch: String,
}

async fn push_test_pr_branch(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    config: &E2eConfig,
    owner: &str,
    repo: &str,
) -> PrBranchSeed {
    let base_branch = repository_default_branch(client, owner, repo).await;
    let remote_url = git_repo_url(config, owner, repo);
    let temp_dir = TempDir::new().expect("temporary directory should be created");
    let worktree = temp_dir.path().join("repo");
    let worktree_str = worktree.to_string_lossy().into_owned();
    let suffix = unique_suffix();
    let feature_branch = format!("e2e-pr-{suffix}");
    let file_name = format!("e2e-pr-{suffix}.txt");

    run_git(
        temp_dir.path(),
        &[
            "clone",
            "--branch",
            &base_branch,
            &remote_url,
            &worktree_str,
        ],
    );
    run_git(&worktree, &["config", "user.name", "GitBucket MCP E2E"]);
    run_git(
        &worktree,
        &["config", "user.email", "gitbucket-mcp-e2e@example.test"],
    );
    run_git(&worktree, &["checkout", "-b", &feature_branch]);

    fs::write(
        worktree.join(&file_name),
        format!("PR E2E change for {feature_branch}\n"),
    )
    .expect("test branch file should be written");

    run_git(&worktree, &["add", &file_name]);
    run_git(
        &worktree,
        &["commit", "-m", &format!("Add {file_name} for PR E2E")],
    );
    run_git(&worktree, &["push", "-u", "origin", &feature_branch]);

    PrBranchSeed {
        base_branch,
        feature_branch,
    }
}

async fn create_test_issue(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    owner: &str,
    repo: &str,
) -> Value {
    let suffix = unique_suffix();
    let title = format!("e2e-issue-{suffix}");
    let body = format!("e2e issue body {suffix}");

    let issue = call_tool_json(
        client,
        "create_issue",
        json!({
            "owner": owner,
            "repo": repo,
            "title": title,
            "body": body,
        }),
    )
    .await;

    assert_eq!(issue["title"].as_str(), Some(title.as_str()));
    assert_eq!(issue["body"].as_str(), Some(body.as_str()));
    assert_eq!(issue["state"].as_str(), Some("open"));

    issue
}

async fn create_test_label(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    owner: &str,
    repo: &str,
) -> Value {
    let suffix = unique_suffix();
    let name = format!("e2e-label-{suffix}");
    let description = format!("e2e label description {suffix}");

    let label = call_tool_json(
        client,
        "create_label",
        json!({
            "owner": owner,
            "repo": repo,
            "name": name,
            "color": "#A1B2C3",
            "description": description,
        }),
    )
    .await;

    assert_eq!(label["name"].as_str(), Some(name.as_str()));
    assert_eq!(label["color"].as_str(), Some("a1b2c3"));

    label
}

async fn create_test_pull_request(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    config: &E2eConfig,
    owner: &str,
    repo: &str,
) -> Value {
    let seed = push_test_pr_branch(client, config, owner, repo).await;
    let suffix = unique_suffix();
    let title = format!("e2e-pr-{suffix}");
    let body = format!("e2e pull request body {suffix}");

    let pr = call_tool_json(
        client,
        "create_pull_request",
        json!({
            "owner": owner,
            "repo": repo,
            "title": title,
            "head": seed.feature_branch,
            "base": seed.base_branch,
            "body": body,
        }),
    )
    .await;

    assert_eq!(pr["title"].as_str(), Some(title.as_str()));
    assert_eq!(pr["body"].as_str(), Some(body.as_str()));
    assert_eq!(pr["state"].as_str(), Some("open"));

    pr
}

async fn create_test_repository(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
) -> (String, String, Value) {
    let authenticated = call_tool_json(client, "get_authenticated_user", json!({})).await;
    let owner = authenticated["login"]
        .as_str()
        .filter(|login| !login.is_empty())
        .expect("authenticated user should have login")
        .to_string();
    let suffix = unique_suffix();
    let name = format!("mcp-e2e-repo-{suffix}");
    let description = format!("repository e2e {suffix}");
    let expected_full_name = format!("{owner}/{name}");

    let repository = call_tool_json(
        client,
        "create_repository",
        json!({
            "name": name,
            "description": description,
            "private": false,
            "auto_init": true,
        }),
    )
    .await;

    assert_eq!(repository["name"].as_str(), Some(name.as_str()));
    assert_eq!(
        repository["full_name"].as_str(),
        Some(expected_full_name.as_str())
    );
    assert_eq!(
        repository["description"].as_str(),
        Some(description.as_str())
    );
    assert_eq!(repository["private"].as_bool(), Some(false));

    (owner, name, repository)
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
#[ignore = "requires GITBUCKET_E2E_URL and GITBUCKET_E2E_TOKEN"]
#[serial]
async fn test_e2e_create_repository() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;

    let (owner, repo, created) = create_test_repository(&client).await;
    let expected_full_name = format!("{owner}/{repo}");
    assert_eq!(
        created["full_name"].as_str(),
        Some(expected_full_name.as_str())
    );
    assert_eq!(created["fork"].as_bool(), Some(false));

    let fetched = call_tool_json(
        &client,
        "get_repository",
        json!({
            "owner": owner,
            "repo": repo,
        }),
    )
    .await;

    assert_eq!(fetched["name"].as_str(), Some(repo.as_str()));
    assert_eq!(
        fetched["full_name"].as_str(),
        Some(expected_full_name.as_str())
    );
    assert_eq!(
        fetched["description"].as_str(),
        created["description"].as_str()
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL and GITBUCKET_E2E_TOKEN"]
#[serial]
async fn test_e2e_list_branches_for_created_repository() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;

    let (owner, repo, created) = create_test_repository(&client).await;
    let default_branch = created["default_branch"]
        .as_str()
        .filter(|branch| !branch.is_empty())
        .expect("auto_init repository should have a default branch")
        .to_string();

    let branches = call_tool_json(
        &client,
        "list_branches",
        json!({
            "owner": owner,
            "repo": repo,
        }),
    )
    .await;

    let branches = branches
        .as_array()
        .expect("list_branches should return an array");
    assert!(
        branches
            .iter()
            .any(|branch| branch["name"].as_str() == Some(default_branch.as_str())),
        "expected branches to contain default branch {default_branch}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_label_lifecycle() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let created = create_test_label(&client, owner, repo).await;
    let name = created["name"]
        .as_str()
        .expect("create_label should return a label name");

    let fetched = call_tool_json(
        &client,
        "get_label",
        json!({
            "owner": owner,
            "repo": repo,
            "name": name,
        }),
    )
    .await;
    assert_eq!(fetched["name"].as_str(), Some(name));

    let labels = call_tool_json(
        &client,
        "list_labels",
        json!({
            "owner": owner,
            "repo": repo,
        }),
    )
    .await;
    assert!(
        labels
            .as_array()
            .expect("list_labels should return an array")
            .iter()
            .any(|label| label["name"] == name),
        "expected created label to appear in list_labels output: {labels}"
    );

    let deleted = call_tool_json(
        &client,
        "delete_label",
        json!({
            "owner": owner,
            "repo": repo,
            "name": name,
        }),
    )
    .await;
    assert_eq!(deleted["deleted"].as_bool(), Some(true));
    assert_eq!(deleted["name"].as_str(), Some(name));

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

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_create_issue() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let issue = create_test_issue(&client, owner, repo).await;
    assert!(issue["number"].as_u64().is_some());

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_update_issue() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let issue = create_test_issue(&client, owner, repo).await;
    let issue_number = issue["number"]
        .as_u64()
        .expect("issue should have a number");
    let suffix = unique_suffix();
    let updated_title = format!("e2e-updated-issue-{suffix}");
    let updated_body = format!("e2e updated body {suffix}");
    let has_web_fallback = config.web_username.is_some() && config.web_password.is_some();
    if has_web_fallback {
        let updated = call_tool(
            &client,
            "update_issue",
            json!({
                "owner": owner,
                "repo": repo,
                "issue_number": issue_number,
                "state": "closed",
                "title": updated_title,
                "body": updated_body,
            }),
        )
        .await;
        let updated = parse_json(&updated);
        assert_eq!(updated["number"].as_u64(), Some(issue_number));
        assert_eq!(updated["state"].as_str(), Some("closed"));
        assert_eq!(updated["title"].as_str(), Some(updated_title.as_str()));
        assert_eq!(updated["body"].as_str(), Some(updated_body.as_str()));
    } else {
        let updated = call_tool(
            &client,
            "update_issue",
            json!({
                "owner": owner,
                "repo": repo,
                "issue_number": issue_number,
                "state": "closed",
                "title": updated_title,
                "body": updated_body,
            }),
        )
        .await;
        if updated.is_error == Some(true) {
            let updated_error = parse_error(&updated);
            assert!(
                updated_error.message.contains("API error (404)")
                    || updated_error
                        .message
                        .contains("Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD"),
                "expected a surfaced compatibility error, got: {:?}",
                updated_error
            );
        } else {
            let updated = parse_json(&updated);
            assert_eq!(updated["number"].as_u64(), Some(issue_number));
            assert_eq!(updated["state"].as_str(), Some("closed"));
            assert_eq!(updated["title"].as_str(), Some(updated_title.as_str()));
            assert_eq!(updated["body"].as_str(), Some(updated_body.as_str()));
        }
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_update_issue_with_title_body_on_web_fallback_instance() {
    let config = E2eConfig::from_env();
    if config.web_username.is_none() || config.web_password.is_none() {
        return;
    }

    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let issue = create_test_issue(&client, owner, repo).await;
    let issue_number = issue["number"]
        .as_u64()
        .expect("issue should have a number");
    let result = call_tool(
        &client,
        "update_issue",
        json!({
            "owner": owner,
            "repo": repo,
            "issue_number": issue_number,
            "state": "closed",
            "title": format!("updated-title-{}", unique_suffix()),
            "body": format!("updated-body-{}", unique_suffix()),
        }),
    )
    .await;
    let updated = parse_json(&result);
    assert_eq!(updated["number"].as_u64(), Some(issue_number));
    assert_eq!(updated["state"].as_str(), Some("closed"));
    assert!(updated["title"]
        .as_str()
        .unwrap_or_default()
        .starts_with("updated-title-"));
    assert!(updated["body"]
        .as_str()
        .unwrap_or_default()
        .starts_with("updated-body-"));

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, and GITBUCKET_E2E_REPO"]
#[serial]
async fn test_e2e_add_issue_comment() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let issue = create_test_issue(&client, owner, repo).await;
    let issue_number = issue["number"]
        .as_u64()
        .expect("issue should have a number");
    let comment_body = format!("e2e issue comment {}", unique_suffix());

    let comment = call_tool_json(
        &client,
        "add_issue_comment",
        json!({
            "owner": owner,
            "repo": repo,
            "issue_number": issue_number,
            "body": comment_body,
        }),
    )
    .await;

    let comment_id = comment["id"].as_u64().expect("comment should have an id");
    assert_eq!(comment["body"].as_str(), Some(comment_body.as_str()));

    let comments = call_tool_json(
        &client,
        "list_issue_comments",
        json!({
            "owner": owner,
            "repo": repo,
            "issue_number": issue_number,
        }),
    )
    .await;

    let comments = comments
        .as_array()
        .expect("issue comments should be returned as an array");
    assert!(comments.iter().any(|entry| {
        entry["id"].as_u64() == Some(comment_id)
            && entry["body"].as_str() == Some(comment_body.as_str())
    }));

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, GITBUCKET_E2E_REPO, GITBUCKET_E2E_GIT_USERNAME, and GITBUCKET_E2E_GIT_PASSWORD"]
#[serial]
async fn test_e2e_create_pull_request() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();
    let default_branch = repository_default_branch(&client, owner, repo).await;

    let pr = create_test_pull_request(&client, &config, owner, repo).await;
    assert!(pr["number"].as_u64().is_some());
    assert_eq!(pr["base"]["ref"].as_str(), Some(default_branch.as_str()));

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, GITBUCKET_E2E_REPO, GITBUCKET_E2E_GIT_USERNAME, and GITBUCKET_E2E_GIT_PASSWORD"]
#[serial]
async fn test_e2e_add_pull_request_comment() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let pr = create_test_pull_request(&client, &config, owner, repo).await;
    let pull_number = pr["number"]
        .as_u64()
        .expect("pull request should have a number");
    let comment_body = format!("e2e pull request comment {}", unique_suffix());

    let comment = call_tool_json(
        &client,
        "add_pull_request_comment",
        json!({
            "owner": owner,
            "repo": repo,
            "pull_number": pull_number,
            "body": comment_body,
        }),
    )
    .await;

    let comment_id = comment["id"].as_u64().expect("comment should have an id");
    assert_eq!(comment["body"].as_str(), Some(comment_body.as_str()));

    let comments = call_tool_json(
        &client,
        "list_issue_comments",
        json!({
            "owner": owner,
            "repo": repo,
            "issue_number": pull_number,
        }),
    )
    .await;

    let comments = comments
        .as_array()
        .expect("pull request comments should be returned as an array");
    assert!(comments.iter().any(|entry| {
        entry["id"].as_u64() == Some(comment_id)
            && entry["body"].as_str() == Some(comment_body.as_str())
    }));

    client.cancel().await.unwrap();
}

#[tokio::test]
#[ignore = "requires GITBUCKET_E2E_URL, GITBUCKET_E2E_TOKEN, GITBUCKET_E2E_OWNER, GITBUCKET_E2E_REPO, GITBUCKET_E2E_GIT_USERNAME, and GITBUCKET_E2E_GIT_PASSWORD"]
#[serial]
async fn test_e2e_merge_pull_request() {
    let config = E2eConfig::from_env();
    let client = spawn_client_and_server(&config).await;
    let (owner, repo) = config.repository_target();

    let pr = create_test_pull_request(&client, &config, owner, repo).await;
    let pull_number = pr["number"]
        .as_u64()
        .expect("pull request should have a number");
    let commit_message = format!("Merge PR E2E {}", unique_suffix());

    let merge_result = call_tool_json(
        &client,
        "merge_pull_request",
        json!({
            "owner": owner,
            "repo": repo,
            "pull_number": pull_number,
            "commit_message": commit_message,
        }),
    )
    .await;

    assert_eq!(merge_result["merged"].as_bool(), Some(true));
    assert!(merge_result["sha"].as_str().is_some());

    let merged_pr = call_tool_json(
        &client,
        "get_pull_request",
        json!({
            "owner": owner,
            "repo": repo,
            "pull_number": pull_number,
        }),
    )
    .await;

    assert_eq!(merged_pr["merged"].as_bool(), Some(true));

    client.cancel().await.unwrap();
}
