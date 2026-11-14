mod common;
use common::TestServer;
use gitbucket_mcp_server::{
    api::profile::ApiProfile,
    error::GbMcpError,
    models::{
        issue::CreateIssue,
        operation::{FieldStatus, OutcomeStatus},
        pull_request::CreatePullRequest,
    },
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, ResponseTemplate,
};
const RAW: &str = "  日本語 **Markdown**  \n\n    code\nlast  \n";
fn change() -> CreateIssue {
    CreateIssue {
        title: "Issue".into(),
        body: Some(RAW.into()),
        labels: None,
        assignees: None,
        milestone: None,
    }
}
fn pr_change(draft: Option<bool>) -> CreatePullRequest {
    CreatePullRequest {
        title: "PR".into(),
        body: Some(RAW.into()),
        head: "feature".into(),
        base: "main".into(),
        draft,
    }
}
fn issue() -> Value {
    json!({"number":7,"title":"Issue","body":RAW,"state":"open","labels":[],"assignees":[],"milestone":null})
}
fn pr(draft: Value) -> Value {
    json!({"number":7,"title":"PR","body":RAW,"state":"open","draft":draft})
}
async fn reference(http: &TestServer, route: &str, value: Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(value))
        .expect(1)
        .mount(&http.mock_server)
        .await;
}
async fn refs(http: &TestServer) {
    reference(
        http,
        "/api/v3/repos/owner/repo/milestones/3",
        json!({"number":3,"title":"Release","state":"closed"}),
    )
    .await;
    reference(
        http,
        "/api/v3/repos/owner/repo/labels/bug",
        json!({"name":"bug","color":"123456"}),
    )
    .await;
    reference(http, "/api/v3/users/alice", json!({"login":"alice"})).await;
}
async fn issue_post(http: &TestServer, result: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(result)
        .expect(1)
        .mount(&http.mock_server)
        .await;
}
async fn connect(
    server: GitBucketMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (a, b) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let running = server.serve(a).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(b).await.unwrap()
}

#[tokio::test]
async fn issue_checks_references_and_verifies_requested_metadata_in_both_profiles() {
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        let http = TestServer::start().await;
        refs(&http).await;
        issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
        let mut stored = issue();
        stored["labels"] = json!([{"name":"bug"}]);
        stored["assignees"] = json!([{"login":"alice"}]);
        stored["milestone"] = json!({"number":3,"title":"Release","state":"closed"});
        reference(&http, "/api/v3/repos/owner/repo/issues/7", stored).await;
        let request = CreateIssue {
            labels: Some(vec!["bug".into()]),
            assignees: Some(vec!["alice".into()]),
            milestone: Some(3),
            ..change()
        };
        let actual = http
            .client()
            .with_api_profile(profile)
            .create_issue("owner", "repo", &request)
            .await
            .unwrap();
        assert_eq!(actual.body.as_deref(), Some(RAW));
        assert_eq!(actual.milestone.unwrap().number, 3);
        assert_eq!(actual.assignees.unwrap()[0].login, "alice");
        let requests = http.mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 5);
        let post = requests
            .iter()
            .find(|r| r.method.as_str() == "POST")
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&post.body).unwrap(),
            json!({"title":"Issue","body":RAW,"labels":["bug"],"assignees":["alice"],"milestone":3})
        );
    }
}

#[tokio::test]
async fn explicit_true_and_false_draft_are_sent_and_verified_in_both_profiles() {
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        for flag in [true, false] {
            let http = TestServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v3/repos/owner/repo/pulls"))
                .and(body_json(
                    json!({"title":"PR","head":"feature","base":"main","body":RAW,"draft":flag}),
                ))
                .respond_with(ResponseTemplate::new(201).set_body_json(pr(json!(!flag))))
                .expect(1)
                .mount(&http.mock_server)
                .await;
            reference(&http, "/api/v3/repos/owner/repo/pulls/7", pr(json!(flag))).await;
            let actual = http
                .client()
                .with_api_profile(profile)
                .create_pull_request("owner", "repo", &pr_change(Some(flag)))
                .await
                .unwrap();
            assert_eq!(actual.draft, Some(flag));
            assert_eq!(actual.body.as_deref(), Some(RAW));
        }
    }
}

#[tokio::test]
async fn omitted_metadata_is_not_serialized_or_inferred_as_an_observation() {
    let http = TestServer::start().await;
    let mut unknown = pr(Value::Null);
    unknown.as_object_mut().unwrap().remove("draft");
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/pulls"))
        .and(body_json(
            json!({"title":"PR","head":"feature","base":"main","body":RAW}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(unknown))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    assert!(http
        .client()
        .create_pull_request("owner", "repo", &pr_change(None))
        .await
        .unwrap()
        .draft
        .is_none());
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    assert_eq!(
        serde_json::to_value(change()).unwrap(),
        json!({"title":"Issue","body":RAW})
    );
}

#[tokio::test]
async fn invalid_milestone_assignees_and_labels_stop_before_http() {
    let http = TestServer::start().await;
    let api = http.client();
    for number in [0, i32::MAX as u64 + 1] {
        assert!(api
            .create_issue(
                "owner",
                "repo",
                &CreateIssue {
                    milestone: Some(number),
                    ..change()
                }
            )
            .await
            .is_err());
    }
    for names in [
        vec!["bad/name".into()],
        vec!["alice".into(), "alice".into()],
        vec!["".into()],
    ] {
        assert!(api
            .create_issue(
                "owner",
                "repo",
                &CreateIssue {
                    assignees: Some(names),
                    ..change()
                }
            )
            .await
            .is_err());
    }
    assert!(api
        .create_issue(
            "owner",
            "repo",
            &CreateIssue {
                labels: Some(vec!["bug".into(), "bug".into()]),
                ..change()
            }
        )
        .await
        .is_err());
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn missing_or_foreign_references_do_not_create_an_issue() {
    for (route, request) in [
        (
            "/api/v3/repos/owner/repo/milestones/3",
            CreateIssue {
                milestone: Some(3),
                ..change()
            },
        ),
        (
            "/api/v3/repos/owner/repo/labels/bug",
            CreateIssue {
                labels: Some(vec!["bug".into()]),
                ..change()
            },
        ),
        (
            "/api/v3/users/alice",
            CreateIssue {
                assignees: Some(vec!["alice".into()]),
                ..change()
            },
        ),
    ] {
        for response in [
            ResponseTemplate::new(404),
            ResponseTemplate::new(200).set_body_json(
                json!({"number":4,"title":"Foreign","state":"open","name":"other","login":"other"}),
            ),
        ] {
            let http = TestServer::start().await;
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(response)
                .expect(1)
                .mount(&http.mock_server)
                .await;
            assert!(http
                .client()
                .create_issue("owner", "repo", &request)
                .await
                .is_err());
            assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn silently_ignored_assignment_is_not_success_and_retains_created_number() {
    let http = TestServer::start().await;
    reference(&http, "/api/v3/users/alice", json!({"login":"alice"})).await;
    issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
    reference(&http, "/api/v3/repos/owner/repo/issues/7", issue()).await;
    let error = http
        .client()
        .create_issue(
            "owner",
            "repo",
            &CreateIssue {
                assignees: Some(vec!["alice".into()]),
                ..change()
            },
        )
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing context")
    };
    assert_eq!(outcome.status, OutcomeStatus::NotVerified);
    assert_eq!(outcome.target.unwrap().number, Some(7));
    assert_eq!(outcome.fields[0].field, "assignees");
    assert_eq!(outcome.fields[0].status, FieldStatus::Mismatched);
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn missing_milestone_observation_keeps_partial_field_evidence() {
    let http = TestServer::start().await;
    refs(&http).await;
    issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
    let mut stored = issue();
    stored["labels"] = json!([{"name":"bug"}]);
    stored["assignees"] = json!([{"login":"alice"}]);
    reference(&http, "/api/v3/repos/owner/repo/issues/7", stored).await;
    let error = http
        .client()
        .create_issue(
            "owner",
            "repo",
            &CreateIssue {
                labels: Some(vec!["bug".into()]),
                assignees: Some(vec!["alice".into()]),
                milestone: Some(3),
                ..change()
            },
        )
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing context")
    };
    assert_eq!(outcome.status, OutcomeStatus::Partial);
    assert_eq!(outcome.target.unwrap().number, Some(7));
    assert_eq!(
        outcome
            .fields
            .iter()
            .find(|f| f.field == "milestone")
            .unwrap()
            .status,
        FieldStatus::Unknown
    );
    assert_eq!(
        outcome
            .fields
            .iter()
            .find(|f| f.field == "assignees")
            .unwrap()
            .status,
        FieldStatus::Verified
    );
}

#[tokio::test]
async fn unknown_and_mismatching_draft_are_distinct_after_creation() {
    for (value, status) in [
        (Value::Null, OutcomeStatus::Unknown),
        (json!(false), OutcomeStatus::NotVerified),
    ] {
        let http = TestServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v3/repos/owner/repo/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(pr(json!(true))))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        reference(&http, "/api/v3/repos/owner/repo/pulls/7", pr(value)).await;
        let error = http
            .client()
            .create_pull_request("owner", "repo", &pr_change(Some(true)))
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing context")
        };
        assert_eq!(outcome.status, status);
        assert_eq!(outcome.target.unwrap().number, Some(7));
    }
}

#[tokio::test]
async fn postwrite_get_errors_keep_the_creation_id_and_do_not_retry() {
    for status in [401, 403, 404, 500] {
        let http = TestServer::start().await;
        issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/issues/7"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let error = http
            .client()
            .create_issue(
                "owner",
                "repo",
                &CreateIssue {
                    labels: Some(vec![]),
                    ..change()
                },
            )
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing context")
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        assert_eq!(outcome.target.unwrap().number, Some(7));
    }
}

#[tokio::test]
async fn initial_creation_rejections_do_not_invent_a_created_id() {
    for status in [401, 403, 404, 422] {
        let http = TestServer::start().await;
        issue_post(&http, ResponseTemplate::new(status)).await;
        let error = http
            .standard_client()
            .create_issue(
                "owner",
                "repo",
                &CreateIssue {
                    labels: Some(vec![]),
                    ..change()
                },
            )
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing context")
        };
        assert_eq!(outcome.status, OutcomeStatus::NotApplied);
        assert!(outcome.target.unwrap().number.is_none());
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn changed_get_identity_keeps_both_intended_and_observed_numbers() {
    let http = TestServer::start().await;
    issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
    let mut foreign = issue();
    foreign["number"] = json!(8);
    reference(&http, "/api/v3/repos/owner/repo/issues/7", foreign).await;
    let error = http
        .client()
        .create_issue(
            "owner",
            "repo",
            &CreateIssue {
                labels: Some(vec![]),
                ..change()
            },
        )
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing context")
    };
    let target = outcome.target.unwrap();
    assert_eq!(target.number, Some(7));
    assert_eq!(target.observed_number, Some(8));
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
}

#[tokio::test]
async fn empty_arrays_are_verified_and_stay_different_from_missing_arrays() {
    for missing in [true, false] {
        let http = TestServer::start().await;
        issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
        let mut observed = issue();
        if missing {
            observed.as_object_mut().unwrap().remove("assignees");
        }
        reference(&http, "/api/v3/repos/owner/repo/issues/7", observed).await;
        let result = http
            .client()
            .create_issue(
                "owner",
                "repo",
                &CreateIssue {
                    assignees: Some(vec![]),
                    ..change()
                },
            )
            .await;
        if missing {
            let GbMcpError::OperationFailed { outcome, .. } = result.unwrap_err() else {
                panic!("missing context")
            };
            assert_eq!(outcome.status, OutcomeStatus::Unknown);
        } else {
            assert_eq!(result.unwrap().assignees.unwrap().len(), 0);
        }
    }
}

#[tokio::test]
async fn unsupported_creation_fields_are_rejected_by_models_and_mcp() {
    for field in ["assignees", "milestone", "maintainer_can_modify"] {
        let mut args = json!({"title":"PR","head":"feature","base":"main"});
        args[field] = json!(1);
        assert!(serde_json::from_value::<CreatePullRequest>(args).is_err());
    }
    assert!(serde_json::from_value::<CreateIssue>(json!({"title":"Issue","draft":true})).is_err());
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for (tool, args) in [
        (
            "create_pull_request",
            json!({"owner":"owner","repo":"repo","title":"PR","head":"feature","base":"main","milestone":3}),
        ),
        (
            "create_issue",
            json!({"owner":"owner","repo":"repo","title":"Issue","draft":true}),
        ),
        (
            "update_pull_request",
            json!({"owner":"owner","repo":"repo","pull_number":7,"draft":false}),
        ),
        (
            "update_issue",
            json!({"owner":"owner","repo":"repo","issue_number":7,"milestone":3}),
        ),
    ] {
        assert!(client
            .call_tool(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone())
            )
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
async fn mcp_normalizes_assignee_identifiers_and_preserves_content_and_milestone() {
    let http = TestServer::start().await;
    refs(&http).await;
    issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
    let mut actual = issue();
    actual["labels"] = json!([{"name":"bug"}]);
    actual["assignees"] = json!([{"login":"alice"}]);
    actual["milestone"] = json!({"number":3,"title":"Release","state":"closed"});
    reference(&http, "/api/v3/repos/owner/repo/issues/7", actual).await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let result=client.call_tool(CallToolRequestParams::new("create_issue").with_arguments(json!({"owner":" owner ","repo":" repo ","title":" Issue ","body":RAW,"assignees":[" alice "],"labels":["bug"],"milestone":3}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.structured_content.unwrap()["milestone"]["number"], 3);
    let requests = http.mock_server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    let sent: Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(sent["assignees"], json!(["alice"]));
    assert_eq!(sent["body"], RAW);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn metadata_verification_deadline_keeps_the_created_issue_number() {
    let http = TestServer::start().await;
    issue_post(&http, ResponseTemplate::new(201).set_body_json(issue())).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(issue())
                .set_delay(std::time::Duration::from_millis(1000)),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let server = GitBucketMcpServer::with_api_and_timeout(
        std::sync::Arc::new(http.client()),
        std::time::Duration::from_millis(200),
    )
    .unwrap();
    let client = connect(server).await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("create_issue").with_arguments(
                json!({"owner":"owner","repo":"repo","title":"Issue","labels":[]})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    let error = result.structured_content.unwrap();
    assert_eq!(error["kind"], "operation_unknown");
    assert_eq!(error["operation"]["target"]["number"], 7);
    assert_eq!(error["cause"]["kind"], "timeout_error");
    client.cancel().await.unwrap();
}
