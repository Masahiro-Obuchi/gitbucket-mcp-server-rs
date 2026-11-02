use gitbucket_mcp_server::{
    api::{
        client::GitBucketClient,
        profile::{ApiProfile, Operation, Route},
    },
    error::GbMcpError,
    models::{
        issue::UpdateIssue,
        label::{CreateLabel, UpdateLabel},
        milestone::CreateMilestone,
        pull_request::UpdatePullRequest,
    },
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer, mode: ApiProfile) -> GitBucketClient {
    GitBucketClient::new_with_web_auth(
        &server.uri(),
        "fake-token",
        false,
        Some("user"),
        Some("fake-password"),
    )
    .unwrap()
    .with_api_profile(mode)
}
fn rename() -> UpdateLabel {
    UpdateLabel {
        new_name: Some("new-name".into()),
        color: Some("ABCDEF".into()),
    }
}
fn recolor() -> UpdateLabel {
    UpdateLabel {
        new_name: None,
        color: Some("ABCDEF".into()),
    }
}
fn pr_update() -> UpdatePullRequest {
    UpdatePullRequest {
        title: None,
        body: None,
        state: Some("closed".into()),
        base: None,
    }
}
fn issue_update() -> UpdateIssue {
    UpdateIssue {
        title: Some("new title".into()),
        body: None,
        state: None,
    }
}
async fn current_label(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/labels/old-name"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":"123456"})),
        )
        .mount(server)
        .await;
}
async fn sign_in(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/home"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/home"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
}

#[test]
fn profiles_express_the_supported_contract_without_legacy_aliases() {
    let standard = ApiProfile::Standard;
    for op in [
        Operation::IssueUpdate,
        Operation::LabelSameNameUpdate,
        Operation::MilestoneRestrictedTitle,
    ] {
        assert_eq!(standard.capability(op).route, Route::Web);
    }
    for op in [
        Operation::PullRequestUpdate,
        Operation::LabelRename,
        Operation::MilestoneWrite,
        Operation::MilestoneDelete,
    ] {
        assert_eq!(standard.capability(op).route, Route::Rest);
    }
    assert!(standard
        .capability(Operation::IssueUpdate)
        .evidence
        .contains("4.44.0 and 4.48.0"));
    assert_eq!(ApiProfile::default(), ApiProfile::RestOnly);
    assert_eq!(
        ApiProfile::RestOnly
            .capability(Operation::IssueUpdate)
            .route,
        Route::Rest
    );
    for value in ["rest-only", "standard"] {
        assert!(value.parse::<ApiProfile>().is_ok());
    }
    for value in [
        "legacy-404",
        "gitbucket-4.44.0",
        "gitbucket-4.48.0",
        "fake-secret-mode",
    ] {
        assert!(!value
            .parse::<ApiProfile>()
            .unwrap_err()
            .to_string()
            .contains(value));
    }
}

#[tokio::test]
async fn unknown_404_preserves_error_without_preflight_or_web_even_with_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let client = client(&server, ApiProfile::RestOnly);
    let results = [
        client
            .update_pull_request("owner", "repo", 1, &pr_update())
            .await
            .unwrap_err(),
        client
            .update_issue("owner", "repo", 1, &issue_update())
            .await
            .unwrap_err(),
        client
            .update_label("owner", "repo", "old-name", &rename())
            .await
            .unwrap_err(),
        client
            .create_milestone(
                "owner",
                "repo",
                &CreateMilestone {
                    title: "日本語".into(),
                    description: None,
                    due_on: None,
                },
            )
            .await
            .unwrap_err(),
        client
            .delete_milestone("owner", "repo", 1)
            .await
            .unwrap_err(),
    ];
    for result in results {
        assert!(matches!(result, GbMcpError::Api { status: 404, .. }));
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    assert!(requests
        .iter()
        .all(|r| r.url.path().starts_with("/api/v3/")));
}

#[tokio::test]
async fn standard_profiles_keep_rest_for_pr_rename_and_normal_milestone_errors() {
    for mode in [ApiProfile::Standard] {
        for status in [401, 403, 404, 409, 422] {
            let server = MockServer::start().await;
            Mock::given(method("PATCH"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let c = client(&server, mode);
            for error in [
                c.update_pull_request("owner", "repo", 1, &pr_update())
                    .await
                    .unwrap_err(),
                c.update_label("owner", "repo", "old-name", &rename())
                    .await
                    .unwrap_err(),
                c.create_milestone(
                    "owner",
                    "repo",
                    &CreateMilestone {
                        title: "title+1".into(),
                        description: None,
                        due_on: None,
                    },
                )
                .await
                .unwrap_err(),
            ] {
                assert!(matches!(error,GbMcpError::Api{status:s,..} if s==status));
            }
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 3);
            assert!(requests
                .iter()
                .all(|r| r.url.path().starts_with("/api/v3/")));
        }
    }
}

#[tokio::test]
async fn label_rename_stays_rest_and_ignored_responses_are_not_success() {
    for mode in [ApiProfile::RestOnly, ApiProfile::Standard] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/labels/old-name"))
            .and(body_json(json!({"name":"new-name","color":"abcdef"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"name":"new-name","color":"abcdef"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            client(&server, mode)
                .update_label("owner", "repo", "old-name", &rename())
                .await
                .unwrap()
                .name,
            "new-name"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server, ApiProfile::Standard)
            .update_label("owner", "repo", "old-name", &rename())
            .await,
        Err(GbMcpError::OperationNotVerified { .. })
    ));
}

#[tokio::test]
async fn same_name_color_uses_web_only_for_selected_contract_and_verifies_result() {
    for mode in [ApiProfile::Standard] {
        for applies in [true, false] {
            let server = MockServer::start().await;
            let changed = Arc::new(AtomicBool::new(false));
            let read = changed.clone();
            Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/labels/old-name"))
                .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":if read.load(Ordering::SeqCst){"abcdef"}else{"123456"}}))).mount(&server).await;
            sign_in(&server).await;
            Mock::given(method("GET")).and(path("/owner/repo/issues/labels"))
                .respond_with(ResponseTemplate::new(200).set_body_string(r#"<tr id="label-row-12"><a href="/owner/repo/issues?labels=old-name">old-name</a></tr>"#)).mount(&server).await;
            Mock::given(method("POST"))
                .and(path("/owner/repo/issues/labels/12/edit"))
                .respond_with(move |_: &wiremock::Request| {
                    changed.store(applies, Ordering::SeqCst);
                    ResponseTemplate::new(200).set_body_string("form page")
                })
                .expect(1)
                .mount(&server)
                .await;
            let result = client(&server, mode)
                .update_label("owner", "repo", "old-name", &recolor())
                .await;
            if applies {
                assert_eq!(result.unwrap().color.as_deref(), Some("abcdef"));
            } else {
                assert!(matches!(
                    result,
                    Err(GbMcpError::OperationNotVerified { .. })
                ));
            }
            assert!(!server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.method.as_str() == "PATCH"));
        }
    }
}

#[tokio::test]
async fn unverified_color_422_is_not_a_reason_to_try_web() {
    for mode in [ApiProfile::RestOnly] {
        let server = MockServer::start().await;
        current_label(&server).await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(422))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            client(&server, mode)
                .update_label("owner", "repo", "old-name", &recolor())
                .await,
            Err(GbMcpError::Api { status: 422, .. })
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn label_write_schema_omits_description_and_rejects_extra_arguments_before_http() {
    use rmcp::{
        model::{CallToolRequestParams, ErrorCode},
        ServiceExt,
    };
    let http = MockServer::start().await;
    let (server_transport, client_transport) = tokio::io::duplex(16384);
    let server =
        gitbucket_mcp_server::server::GitBucketMcpServer::new(client(&http, ApiProfile::Standard));
    tokio::spawn(async move {
        let running = server.serve(server_transport).await.unwrap();
        let _ = running.waiting().await;
    });
    let peer = ().serve(client_transport).await.unwrap();
    let tools = peer.list_all_tools().await.unwrap();
    for tool_name in ["create_label", "update_label"] {
        let tool = tools.iter().find(|t| t.name == tool_name).unwrap();
        assert!(tool.input_schema["properties"].get("description").is_none());
        assert_eq!(tool.input_schema["additionalProperties"], json!(false));
        for value in [json!(""), json!("ignored"), json!(null)] {
            let mut arguments =
                json!({"owner":"owner","repo":"repo","color":"123456","description":value});
            if tool_name == "create_label" {
                arguments["name"] = json!("label");
            } else {
                arguments["name"] = json!("old-name");
                arguments["new_name"] = json!("new-name");
            }
            let error = peer
                .call_tool(
                    CallToolRequestParams::new(tool_name)
                        .with_arguments(arguments.as_object().unwrap().clone()),
                )
                .await
                .unwrap_err();
            assert!(
                matches!(error,rmcp::service::ServiceError::McpError(error) if error.code==ErrorCode::INVALID_PARAMS)
            );
        }
        let mut arguments = json!({"owner":"owner","repo":"repo","name":"label","color":"123456","colour":"ignored"});
        if tool_name == "update_label" {
            arguments["new_name"] = json!("new-name");
        }
        assert!(peer
            .call_tool(
                CallToolRequestParams::new(tool_name)
                    .with_arguments(arguments.as_object().unwrap().clone())
            )
            .await
            .is_err());
    }
    assert!(http.received_requests().await.unwrap().is_empty());
    peer.cancel().await.unwrap();
}

#[tokio::test]
async fn normalized_color_noop_and_missing_web_credentials_do_not_write() {
    let server = MockServer::start().await;
    current_label(&server).await;
    let c = GitBucketClient::new(&server.uri(), "fake-token")
        .unwrap()
        .with_api_profile(ApiProfile::Standard);
    let noop = UpdateLabel {
        new_name: Some("old-name".into()),
        color: Some("#123456".into()),
    };
    assert_eq!(
        c.update_label("owner", "repo", "old-name", &noop)
            .await
            .unwrap()
            .color
            .as_deref(),
        Some("123456")
    );
    assert!(c
        .update_label("owner", "repo", "old-name", &recolor())
        .await
        .unwrap_err()
        .to_string()
        .contains("GITBUCKET_USERNAME"));
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method.as_str() == "GET"));
}

#[tokio::test]
async fn known_issue_updates_use_web_directly_without_patch_or_dummy_comment() {
    let server = MockServer::start().await;
    let changed = Arc::new(AtomicBool::new(false));
    let read = changed.clone();
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"number":1,"title":if read.load(Ordering::SeqCst){"new title"}else{"old"},"state":"open"}))).mount(&server).await;
    sign_in(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(move |_: &wiremock::Request| {
            changed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server, ApiProfile::Standard)
            .update_issue("owner", "repo", 1, &issue_update())
            .await
            .unwrap()
            .title,
        "new title"
    );
    let requests = server.received_requests().await.unwrap();
    assert!(!requests
        .iter()
        .any(|r| r.method.as_str() == "PATCH" || r.url.path().ends_with("/comments")));
}

#[tokio::test]
async fn known_restricted_milestone_titles_select_web_before_rest_write() {
    let server = MockServer::start().await;
    let changed = Arc::new(AtomicBool::new(false));
    let read = changed.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(if read.load(Ordering::SeqCst) {
                json!([{"number":1,"title":"日本語","state":"open"}])
            } else {
                json!([])
            })
        })
        .mount(&server)
        .await;
    sign_in(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/new"))
        .respond_with(move |_: &wiremock::Request| {
            changed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server, ApiProfile::Standard)
            .create_milestone(
                "owner",
                "repo",
                &CreateMilestone {
                    title: "日本語".into(),
                    description: None,
                    due_on: None
                }
            )
            .await
            .unwrap()
            .number,
        1
    );
    assert!(!server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.method.as_str() == "POST" && r.url.path().starts_with("/api/v3/")));
}

#[tokio::test]
async fn a_wrong_label_snapshot_is_not_used_for_a_web_write() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/labels/old-name"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name":"another-label","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(client(&server, ApiProfile::Standard)
        .update_label("owner", "repo", "old-name", &recolor())
        .await
        .is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn label_creation_verifies_name_and_color_instead_of_assuming_http_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"name":"ignored-name","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server, ApiProfile::RestOnly)
            .create_label(
                "owner",
                "repo",
                &CreateLabel {
                    name: "label".into(),
                    color: "abcdef".into(),
                }
            )
            .await,
        Err(GbMcpError::OperationNotVerified { .. })
    ));
}
