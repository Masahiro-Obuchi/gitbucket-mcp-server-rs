use std::collections::BTreeSet;

use gitbucket_mcp_server::api::client::GitBucketClient;
use gitbucket_mcp_server::server::GitBucketMcpServer;
use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt;
use serde_json::json;

async fn spawn_client_and_server() -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    tokio::spawn(async move {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client)
            .serve(server_transport)
            .await
            .unwrap();
        server.waiting().await.unwrap();
    });

    ().serve(client_transport).await.unwrap()
}

fn first_text(result: &rmcp::model::CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("expected text tool result")
}

#[tokio::test]
async fn test_mcp_lists_all_expected_tools() {
    let client = spawn_client_and_server().await;

    let tools = client.list_all_tools().await.unwrap();
    let tool_names: BTreeSet<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();

    let expected = BTreeSet::from([
        "add_issue_comment",
        "add_pull_request_comment",
        "create_issue",
        "create_pull_request",
        "create_repository",
        "fork_repository",
        "get_authenticated_user",
        "get_issue",
        "get_pull_request",
        "get_repository",
        "get_user",
        "list_branches",
        "list_issue_comments",
        "list_issues",
        "list_pull_requests",
        "list_repositories",
        "merge_pull_request",
        "update_issue",
    ]);

    assert_eq!(tool_names, expected);

    let get_user = tools
        .iter()
        .find(|tool| tool.name == "get_user")
        .expect("get_user tool should be listed");
    let required = get_user
        .input_schema
        .get("required")
        .and_then(|value| value.as_array())
        .expect("required schema array");
    assert!(required.iter().any(|value| value == "username"));

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_returns_validation_error_for_blank_username() {
    let client = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_user").with_arguments(
                json!({
                    "username": "   "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(first_text(&result), "Error: username must not be empty");

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_returns_validation_error_for_empty_issue_update() {
    let client = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("update_issue").with_arguments(
                json!({
                    "owner": "owner",
                    "repo": "repo",
                    "issue_number": 42
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        first_text(&result),
        "Error: at least one of state, title, or body must be provided"
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_rejects_invalid_state_before_api_call() {
    let client = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_pull_requests").with_arguments(
                json!({
                    "owner": "owner",
                    "repo": "repo",
                    "state": "merged"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        first_text(&result),
        "Error: state must be one of: open, closed, all"
    );

    client.cancel().await.unwrap();
}
