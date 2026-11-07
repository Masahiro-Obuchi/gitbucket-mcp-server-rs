#[allow(dead_code)]
mod common;

use common::TestServer;
use gitbucket_mcp_server::{
    api::profile::ApiProfile,
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::ServerHandler;
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Map, Value};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

async fn connect(
    server: GitBucketMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(client_transport).await.unwrap()
}

fn valid_args(tool: &rmcp::model::Tool) -> Map<String, Value> {
    let mut args = Map::new();
    for field in tool.input_schema["required"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        let value = if field.ends_with("_number") {
            json!(1)
        } else if field == "color" {
            json!("abcdef")
        } else {
            json!("example")
        };
        args.insert(field.into(), value);
    }
    if tool.name.starts_with("update_") {
        let field = if tool.name == "update_label" {
            "color"
        } else {
            "title"
        };
        args.insert(
            field.into(),
            json!(if field == "color" {
                "abcdef"
            } else {
                "changed"
            }),
        );
    }
    if tool.name == "add_pull_request_review_comment" {
        args.insert("commit".into(), json!("b".repeat(40)));
        args.insert("head_guard".into(), json!("preflight"));
        args.insert("new_line".into(), json!(2));
    }
    args
}

#[tokio::test]
async fn every_tool_rejects_unknown_arguments_before_http() {
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for tool in client.list_all_tools().await.unwrap() {
        assert_eq!(
            tool.input_schema.get("additionalProperties"),
            Some(&json!(false)),
            "{}",
            tool.name
        );
        let mut args = valid_args(&tool);
        args.insert("unexpected_argument".into(), json!(true));
        let result = client
            .call_tool(CallToolRequestParams::new(tool.name.clone()).with_arguments(args))
            .await;
        assert!(
            matches!(result, Err(rmcp::service::ServiceError::McpError(ref error))
            if error.code == rmcp::model::ErrorCode::INVALID_PARAMS),
            "{}: {result:?}",
            tool.name
        );
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn blank_comments_are_rejected_without_http() {
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for (tool, number) in [
        ("add_issue_comment", "issue_number"),
        ("add_pull_request_comment", "pull_number"),
    ] {
        let mut args = json!({"owner":"owner","repo":"repo","body":" \t\n"});
        args[number] = json!(7);
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["kind"],
            "validation_error"
        );
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn optional_markdown_and_commit_messages_reach_http_unchanged() {
    let http = TestServer::start().await;
    let text = "    indented code\n\nA hard break  \n";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"number":7,"title":"v1.0","state":"open","description":"old"}),
            ),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    for verb in ["POST", "PATCH", "PUT"] {
        Mock::given(method(verb)).respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let value = if request.url.path().ends_with("/merge") {
                json!({"merged":true,"sha":"merged-sha","message":"merged"})
            } else if request.url.path().ends_with("/user/repos") {
                json!({"name":"repo","full_name":"owner/repo","private":false,"description":body["description"]})
            } else {
                json!({"number":7,"title":body["title"],"state":"open","body":body["body"],"description":body["description"],"due_on":body["due_on"]})
            };
            ResponseTemplate::new(200).set_body_json(value)
        }).mount(&http.mock_server).await;
    }
    let client = connect(GitBucketMcpServer::new(
        http.client().with_api_profile(ApiProfile::RestOnly),
    ))
    .await;
    let cases = [
        (
            "create_pull_request",
            json!({"owner":"owner","repo":"repo","title":"title","head":"feature","base":"main","body":text}),
        ),
        (
            "update_pull_request",
            json!({"owner":"owner","repo":"repo","pull_number":7,"title":"title","body":text}),
        ),
        (
            "update_issue",
            json!({"owner":"owner","repo":"repo","issue_number":7,"title":"title","body":text}),
        ),
        (
            "create_milestone",
            json!({"owner":"owner","repo":"repo","title":"v1.0","description":text}),
        ),
        (
            "update_milestone",
            json!({"owner":"owner","repo":"repo","milestone_number":7,"description":text}),
        ),
        (
            "create_repository",
            json!({"name":"repo","description":text}),
        ),
        (
            "merge_pull_request",
            json!({"owner":"owner","repo":"repo","pull_number":7,"commit_message":text}),
        ),
    ];
    for (tool, args) in &cases {
        let result = client
            .call_tool(
                CallToolRequestParams::new(*tool).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false), "{tool}: {result:?}");
    }
    let requests = http.mock_server.received_requests().await.unwrap();
    let writes: Vec<_> = requests.iter().filter(|r| r.method != "GET").collect();
    assert_eq!(writes.len(), cases.len());
    for request in writes {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let field = if request.url.path().ends_with("/merge") {
            "commit_message"
        } else if request.url.path().contains("/milestones")
            || request.url.path().ends_with("/user/repos")
        {
            "description"
        } else {
            "body"
        };
        assert_eq!(body[field], text, "{}", request.url.path());
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn lossy_issue_web_content_is_rejected_before_any_other_update() {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/7"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"number":7,"title":"original","body":"original","state":"open"}),
            ),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.standard_client())).await;
    let result = client.call_tool(CallToolRequestParams::new("update_issue").with_arguments(json!({
        "owner":"owner","repo":"repo","issue_number":7,"title":"must not apply", "state":"closed",
        "body":"    code\n"
    }).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "unsupported_error"
    );
    let requests = http.mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn unchanged_issue_web_body_skips_the_lossy_content_form() {
    let http = TestServer::start().await;
    let text = "    unchanged code\n";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":7,"title":"original","body":text,"state":"open"})),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.standard_client())).await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("update_issue").with_arguments(
                json!({
                    "owner":"owner","repo":"repo","issue_number":7,"body":text
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.structured_content.unwrap()["body"], text);
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    client.cancel().await.unwrap();
}

fn assert_retained(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(actual.get(key).is_some(), "missing {key}");
                assert_retained(&actual[key], value);
            }
        }
        Value::Array(rows) => {
            assert_eq!(actual.as_array().unwrap().len(), rows.len());
            for (a, e) in actual.as_array().unwrap().iter().zip(rows) {
                assert_retained(a, e);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[tokio::test]
async fn issue_and_pr_contract_fields_survive_get_and_list() {
    for (fixture, resource, get_tool, list_tool, result_key, number_key) in [
        (
            include_str!("fixtures/gitbucket/issue.json"),
            "issues",
            "get_issue",
            "list_issues",
            "issues",
            "issue_number",
        ),
        (
            include_str!("fixtures/gitbucket/pull_request.json"),
            "pulls",
            "get_pull_request",
            "list_pull_requests",
            "pull_requests",
            "pull_number",
        ),
    ] {
        let http = TestServer::start().await;
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let endpoint = format!("/api/v3/repos/owner/repo/{resource}");
        Mock::given(method("GET"))
            .and(path(format!("{endpoint}/{}", expected["number"])))
            .respond_with(ResponseTemplate::new(200).set_body_json(expected.clone()))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        for page in ["1", "2"] {
            let rows = if page == "1" {
                json!([expected])
            } else {
                json!([])
            };
            Mock::given(method("GET"))
                .and(path(endpoint.clone()))
                .and(wiremock::matchers::query_param("page", page))
                .respond_with(ResponseTemplate::new(200).set_body_json(rows))
                .expect(1)
                .mount(&http.mock_server)
                .await;
        }
        let client = connect(GitBucketMcpServer::new(http.client())).await;
        for (tool, mut args) in [
            (get_tool, json!({"owner":"owner","repo":"repo"})),
            (
                list_tool,
                json!({"owner":"owner","repo":"repo","state":"all","view":"full"}),
            ),
        ] {
            if tool == get_tool {
                args[number_key] = expected["number"].clone();
            }
            let result = client
                .call_tool(
                    CallToolRequestParams::new(tool)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            let output = result.structured_content.unwrap();
            let actual = if tool == get_tool {
                &output
            } else {
                &output[result_key][0]
            };
            assert_retained(actual, &expected);
            if resource == "issues" {
                assert!(actual["closed_at"].is_null());
            }
        }
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn absent_metadata_stays_unknown_and_empty_or_false_values_stay_explicit() {
    for supplied in [false, true] {
        let http = TestServer::start().await;
        let mut pr = json!({"number":7,"title":"PR","state":"open"});
        let mut issue = json!({"number":7,"title":"Issue","state":"closed","updated_at":"2026-11-04T12:00:00Z"});
        let mut repo = json!({"name":"repo","full_name":"owner/repo","private":false});
        if supplied {
            for (key, value) in [
                ("draft", json!(true)),
                ("mergeable", json!(false)),
                ("merged", json!(false)),
                ("labels", json!([])),
                ("assignees", json!([])),
            ] {
                pr[key] = value;
            }
            issue["labels"] = json!([]);
            issue["assignees"] = json!([]);
            issue["closed_at"] = json!("2026-10-01T00:00:00Z");
            issue["pull_request"] = json!({
                "url":"https://gitbucket.example/api/v3/repos/owner/repo/pulls/7",
                "html_url":"https://gitbucket.example/owner/repo/pull/7"
            });
            repo["fork"] = json!(false);
        }
        for (endpoint, value) in [
            ("/repos/owner/repo/pulls/7", pr),
            ("/repos/owner/repo/issues/7", issue),
            ("/repos/owner/repo", repo),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3{endpoint}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(value))
                .mount(&http.mock_server)
                .await;
        }
        let client = connect(GitBucketMcpServer::new(http.client())).await;
        for (tool, args, fields) in [
            (
                "get_pull_request",
                json!({"owner":"owner","repo":"repo","pull_number":7}),
                vec!["draft", "mergeable", "merged", "labels", "assignees"],
            ),
            (
                "get_issue",
                json!({"owner":"owner","repo":"repo","issue_number":7}),
                vec!["labels", "assignees", "closed_at", "pull_request"],
            ),
            (
                "get_repository",
                json!({"owner":"owner","repo":"repo"}),
                vec!["fork"],
            ),
        ] {
            let result = client
                .call_tool(
                    CallToolRequestParams::new(tool)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            let value = result.structured_content.unwrap();
            for field in fields {
                if !supplied {
                    assert!(value.get(field).unwrap().is_null(), "{tool}.{field}");
                } else if matches!(field, "labels" | "assignees") {
                    assert_eq!(value[field], json!([]));
                } else if field == "closed_at" {
                    assert_eq!(value[field], "2026-10-01T00:00:00Z");
                } else if field == "draft" {
                    assert_eq!(value[field], true);
                } else if field == "pull_request" {
                    assert_eq!(
                        value[field],
                        json!({
                            "url":"https://gitbucket.example/api/v3/repos/owner/repo/pulls/7",
                            "html_url":"https://gitbucket.example/owner/repo/pull/7"
                        })
                    );
                } else {
                    assert_eq!(value[field], false);
                }
            }
        }
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn annotations_describe_reads_additions_and_destructive_writes() {
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for tool in client.list_all_tools().await.unwrap() {
        let read = tool.name.starts_with("get_") || tool.name.starts_with("list_");
        let destructive = tool.name.starts_with("update_")
            || tool.name.starts_with("delete_")
            || tool.name == "merge_pull_request";
        let annotations = tool.annotations.as_ref().expect("every tool has hints");
        assert_eq!(annotations.read_only_hint, Some(read), "{}", tool.name);
        assert_eq!(
            annotations.destructive_hint,
            Some(destructive),
            "{}",
            tool.name
        );
        assert_eq!(
            annotations.idempotent_hint,
            Some(read || tool.name.starts_with("delete_")),
            "{}",
            tool.name
        );
        assert_eq!(annotations.open_world_hint, Some(true));
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn read_only_mode_hides_and_blocks_every_write_including_direct_handlers() {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"user"})))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let normal = GitBucketMcpServer::new(http.client());
    let server = normal.clone().with_access_mode(AccessMode::ReadOnly);
    let client = connect(server.clone()).await;
    let normal_client = connect(normal).await;
    let all = normal_client.list_all_tools().await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 16);
    assert!(tools
        .iter()
        .all(|t| t.name.starts_with("get_") || t.name.starts_with("list_")));
    for tool in all
        .iter()
        .filter(|t| !(t.name.starts_with("get_") || t.name.starts_with("list_")))
    {
        assert!(server.get_tool(&tool.name).is_none());
        let args = Value::Object(valid_args(tool));
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool.name.clone())
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["kind"],
            "read_only_error"
        );
        macro_rules! direct {
            ($method:ident) => {
                server
                    .$method(Parameters(serde_json::from_value(args.clone()).unwrap()))
                    .await
                    .unwrap()
            };
        }
        let result = match tool.name.as_ref() {
            "create_repository" => direct!(create_repository),
            "fork_repository" => direct!(fork_repository),
            "create_label" => direct!(create_label),
            "update_label" => direct!(update_label),
            "delete_label" => direct!(delete_label),
            "create_milestone" => direct!(create_milestone),
            "update_milestone" => direct!(update_milestone),
            "delete_milestone" => direct!(delete_milestone),
            "create_issue" => direct!(create_issue),
            "update_issue" => direct!(update_issue),
            "add_issue_comment" => direct!(add_issue_comment),
            "create_pull_request" => direct!(create_pull_request),
            "update_pull_request" => direct!(update_pull_request),
            "merge_pull_request" => direct!(merge_pull_request),
            "add_pull_request_comment" => direct!(add_pull_request_comment),
            "add_pull_request_review_comment" => direct!(add_pull_request_review_comment),
            name => panic!("add a direct-handler check for {name}"),
        };
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["kind"],
            "read_only_error"
        );
    }
    let read = client
        .call_tool(CallToolRequestParams::new("get_authenticated_user"))
        .await
        .unwrap();
    assert_eq!(read.is_error, Some(false));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    client.cancel().await.unwrap();
    normal_client.cancel().await.unwrap();
}

#[tokio::test]
async fn inline_arguments_cannot_become_an_ordinary_comment() {
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for (key, value) in [
        ("path", json!("src/main.rs")),
        ("line", json!(42)),
        ("commit_id", json!("abc123")),
    ] {
        let mut args =
            json!({"owner":"owner", "repo":"repo", "pull_number":7, "body":"Review comment"})
                .as_object()
                .unwrap()
                .clone();
        args.insert(key.into(), value);
        assert!(client
            .call_tool(CallToolRequestParams::new("add_pull_request_comment").with_arguments(args))
            .await
            .is_err());
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn issue_and_comment_markdown_reaches_http_unchanged() {
    let http = TestServer::start().await;
    let text = "    indented code\n\nA hard break  \n";
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"number":7,"title":"title","body":text,"state":"open"})),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/issues/7/comments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id":1,"body":text})))
        .expect(2)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for (tool, args) in [
        (
            "create_issue",
            json!({"owner":"owner","repo":"repo","title":"title","body":text}),
        ),
        (
            "add_issue_comment",
            json!({"owner":"owner","repo":"repo","issue_number":7,"body":text}),
        ),
        (
            "add_pull_request_comment",
            json!({"owner":"owner","repo":"repo","pull_number":7,"body":text}),
        ),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
    }
    for request in http.mock_server.received_requests().await.unwrap() {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["body"], text);
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn closed_issue_timestamps_are_never_inferred_after_edits() {
    let http = TestServer::start().await;
    let edited =
        json!({"number":7,"title":"edited","state":"closed","updated_at":"2026-11-04T12:00:00Z"});
    for verb in ["GET", "PATCH"] {
        Mock::given(method(verb))
            .and(path("/api/v3/repos/owner/repo/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(edited.clone()))
            .mount(&http.mock_server)
            .await;
    }
    let client = connect(GitBucketMcpServer::new(
        http.client().with_api_profile(ApiProfile::RestOnly),
    ))
    .await;
    for (tool, args) in [
        (
            "get_issue",
            json!({"owner":"owner","repo":"repo","issue_number":7}),
        ),
        (
            "update_issue",
            json!({"owner":"owner","repo":"repo","issue_number":7,"title":"edited"}),
        ),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.unwrap();
        assert_eq!(value["updated_at"], edited["updated_at"]);
        assert!(value["closed_at"].is_null(), "{tool}: {value}");
    }
    client.cancel().await.unwrap();
}
