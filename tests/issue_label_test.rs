mod common;
use common::TestServer;
use gitbucket_mcp_server::{
    api::profile::ApiProfile,
    error::GbMcpError,
    models::{
        issue_label::LabelNames,
        operation::{OutcomeStatus, ResourceKind},
    },
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, ResponseTemplate,
};
const PARENT: &str = "/api/v3/repos/owner/repo/issues/7";
const LABELS: &str = "/api/v3/repos/owner/repo/issues/7/labels";
fn labels(names: &[&str]) -> Value {
    json!(names.iter().map(|name| json!({"name":name,"color":"123abc","description":null,"url":"https://example.test/label"})).collect::<Vec<_>>())
}
fn request(names: &[&str]) -> LabelNames {
    LabelNames {
        labels: names.iter().map(|s| s.to_string()).collect(),
    }
}
async fn fixture(before: Value, after: ResponseTemplate) -> TestServer {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path(PARENT))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":7,"title":"Issue or PR","state":"open"})),
        )
        .mount(&http.mock_server)
        .await;
    let reads = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path(LABELS))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(before.clone())
            } else {
                after.clone()
            }
        })
        .mount(&http.mock_server)
        .await;
    http
}
async fn write(http: &TestServer, verb: &str, body: Option<Value>, response: ResponseTemplate) {
    let mut mock = Mock::given(method(verb)).and(path(LABELS));
    if let Some(body) = body {
        mock = mock.and(body_json(body));
    }
    mock.respond_with(response)
        .expect(1)
        .mount(&http.mock_server)
        .await;
}
async fn connect(http: &TestServer) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (a, b) = tokio::io::duplex(4096);
    let server = GitBucketMcpServer::new(http.client());
    tokio::spawn(async move {
        let running = server.serve(a).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(b).await.unwrap()
}

#[tokio::test]
async fn list_checks_parent_and_uses_one_unpaged_label_read() {
    let http = fixture(labels(&["bug", "日本語"]), ResponseTemplate::new(500)).await;
    let actual = http
        .client()
        .list_issue_labels("owner", "repo", 7)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        labels(&["bug", "日本語"])
    );
    let calls = http.mock_server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|r| r.url.query().is_none()));
}

#[tokio::test]
async fn add_sends_only_missing_names_and_returns_full_verified_set_in_both_profiles() {
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        let http = fixture(
            labels(&["keep", "bug"]),
            ResponseTemplate::new(200).set_body_json(labels(&["new", "bug", "keep"])),
        )
        .await;
        write(
            &http,
            "POST",
            Some(json!({"labels":["new"]})),
            ResponseTemplate::new(200).set_body_json(labels(&["new"])),
        )
        .await;
        let actual = http
            .client()
            .with_api_profile(profile)
            .add_issue_labels("owner", "repo", 7, &request(&["bug", "new"]))
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            labels(&["new", "bug", "keep"])
        );
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 5);
    }
}

#[tokio::test]
async fn replace_empty_clears_assignments_with_one_put_and_no_definition_delete() {
    let http = fixture(
        labels(&["a", "b"]),
        ResponseTemplate::new(200).set_body_json(labels(&[])),
    )
    .await;
    write(
        &http,
        "PUT",
        Some(json!({"labels":[]})),
        ResponseTemplate::new(200).set_body_json(labels(&[])),
    )
    .await;
    assert!(http
        .client()
        .replace_issue_labels("owner", "repo", 7, &request(&[]))
        .await
        .unwrap()
        .is_empty());
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method.as_str() != "DELETE"));
}

#[tokio::test]
async fn remove_encodes_exact_name_and_ignores_internal_delete_model() {
    let exact = "日本語 /#?% +";
    let http = fixture(
        labels(&["keep", exact]),
        ResponseTemplate::new(200).set_body_json(labels(&["keep"])),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "{LABELS}/%E6%97%A5%E6%9C%AC%E8%AA%9E%20%2F%23%3F%25%20+"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{"labelName":exact,"labelId":17}])),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let actual = http
        .client()
        .remove_issue_label("owner", "repo", 7, exact)
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), labels(&["keep"]));
}

#[tokio::test]
async fn unchanged_add_replace_and_absent_remove_are_read_only_noops() {
    let http = fixture(
        labels(&["same"]),
        ResponseTemplate::new(200).set_body_json(labels(&["same"])),
    )
    .await;
    let api = http.client();
    api.add_issue_labels("owner", "repo", 7, &request(&["same"]))
        .await
        .unwrap();
    api.replace_issue_labels("owner", "repo", 7, &request(&["same"]))
        .await
        .unwrap();
    api.remove_issue_label("owner", "repo", 7, "absent")
        .await
        .unwrap();
    let calls = http.mock_server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 6);
    assert!(calls.iter().all(|r| r.method.as_str() == "GET"));
}

#[tokio::test]
async fn lost_unrelated_label_is_not_a_successful_add() {
    let http = fixture(
        labels(&["keep"]),
        ResponseTemplate::new(200).set_body_json(labels(&["new"])),
    )
    .await;
    write(
        &http,
        "POST",
        Some(json!({"labels":["new"]})),
        ResponseTemplate::new(200).set_body_json(labels(&["new"])),
    )
    .await;
    let error = http
        .client()
        .add_issue_labels("owner", "repo", 7, &request(&["new"]))
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing write context")
    };
    assert_eq!(outcome.status, OutcomeStatus::NotVerified);
    let target = outcome.target.unwrap();
    assert_eq!(target.resource, ResourceKind::IssueLabels);
    assert_eq!(target.number, Some(7));
}

#[tokio::test]
async fn postwrite_read_failure_is_unknown_including_404() {
    for status in [401, 403, 404, 500] {
        let http = fixture(labels(&["old"]), ResponseTemplate::new(status)).await;
        write(
            &http,
            "PUT",
            None,
            ResponseTemplate::new(200).set_body_json(labels(&["new"])),
        )
        .await;
        let error = http
            .client()
            .replace_issue_labels("owner", "repo", 7, &request(&["new"]))
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing write context")
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        assert_eq!(outcome.target.unwrap().number, Some(7));
    }
}

#[tokio::test]
async fn initial_4xx_rejections_do_not_fallback_or_retry() {
    for status in [401, 403, 404, 422] {
        let http = fixture(labels(&[]), ResponseTemplate::new(500)).await;
        write(&http, "POST", None, ResponseTemplate::new(status)).await;
        let error = http
            .standard_client()
            .add_issue_labels("owner", "repo", 7, &request(&["new"]))
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing write context")
        };
        assert_eq!(outcome.status, OutcomeStatus::NotApplied);
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 3);
    }
}

#[tokio::test]
async fn server_failure_after_possible_prefix_has_unknown_outcome_without_retry() {
    let http = fixture(labels(&[]), ResponseTemplate::new(500)).await;
    write(&http, "POST", None, ResponseTemplate::new(500)).await;
    let error = http
        .client()
        .add_issue_labels("owner", "repo", 7, &request(&["first", "second"]))
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing write context")
    };
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn invalid_target_names_and_duplicates_are_rejected_before_http() {
    let http = TestServer::start().await;
    let api = http.client();
    for id in [0, i32::MAX as u64 + 1] {
        assert!(api.list_issue_labels("owner", "repo", id).await.is_err());
        assert!(api
            .add_issue_labels("owner", "repo", id, &request(&["a"]))
            .await
            .is_err());
    }
    for names in [
        vec![" ".to_owned()],
        vec!["a".into(), "a".into()],
        vec!["a\nb".into()],
        vec!["😀".repeat(51)],
    ] {
        assert!(api
            .replace_issue_labels("owner", "repo", 7, &LabelNames { labels: names })
            .await
            .is_err());
    }
    assert!(api
        .add_issue_labels("owner", "repo", 7, &request(&[]))
        .await
        .is_err());
    assert!(api
        .remove_issue_label("owner", "repo", 7, "..")
        .await
        .is_err());
    assert!(api
        .add_issue_labels("../owner", "repo", 7, &request(&["a"]))
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
async fn nonexistent_parent_cannot_get_phantom_assignments_or_create_labels() {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path(PARENT))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    assert!(http
        .client()
        .add_issue_labels("owner", "repo", 7, &request(&["new"]))
        .await
        .is_err());
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn duplicate_source_labels_are_not_a_complete_observation() {
    let http = fixture(labels(&["same", "same"]), ResponseTemplate::new(500)).await;
    assert!(http
        .client()
        .list_issue_labels("owner", "repo", 7)
        .await
        .is_err());
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn foreign_parent_number_stops_before_label_reads_or_writes() {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path(PARENT))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":8,"title":"Other issue","state":"open"})),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    assert!(http
        .client()
        .replace_issue_labels("owner", "repo", 7, &request(&["new"]))
        .await
        .is_err());
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn parent_disappearing_after_put_is_unknown_even_when_empty_set_was_requested() {
    let http = TestServer::start().await;
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path(PARENT))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"number":7,"title":"Issue","state":"open"}))
            } else {
                ResponseTemplate::new(404)
            }
        })
        .expect(2)
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(LABELS))
        .respond_with(ResponseTemplate::new(200).set_body_json(labels(&["old"])))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    write(
        &http,
        "PUT",
        Some(json!({"labels":[]})),
        ResponseTemplate::new(200).set_body_json(labels(&[])),
    )
    .await;
    let error = http
        .client()
        .replace_issue_labels("owner", "repo", 7, &request(&[]))
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing write context")
    };
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
    assert_eq!(outcome.target.unwrap().number, Some(7));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn mcp_labels_are_cached_and_cursor_bound_to_issue_and_tool() {
    let http = fixture(
        labels(&["a", "b", "c"]),
        ResponseTemplate::new(200).set_body_json(labels(&["later"])),
    )
    .await;
    let client = connect(&http).await;
    let args = json!({"owner":"owner","repo":"repo","issue_number":7,"limit":1,"view":"full"});
    let first = client
        .call_tool(
            CallToolRequestParams::new("list_issue_labels")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let first = first.structured_content.unwrap();
    assert_eq!(first["labels"][0]["name"], "a");
    let mut next = args.clone();
    next["cursor"] = first["next_cursor"].clone();
    let mut wrong = next.clone();
    wrong["issue_number"] = json!(8);
    let rejected = client
        .call_tool(
            CallToolRequestParams::new("list_issue_labels")
                .with_arguments(wrong.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true));
    let second = client
        .call_tool(
            CallToolRequestParams::new("list_issue_labels")
                .with_arguments(next.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(second.structured_content.unwrap()["labels"][0]["name"], "b");
    let wrong_tool = json!({"owner":"owner","repo":"repo","limit":1,"view":"full","cursor":first["next_cursor"]});
    let rejected = client
        .call_tool(
            CallToolRequestParams::new("list_labels")
                .with_arguments(wrong_tool.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 2);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_preserves_exact_names_and_returns_full_verified_labels() {
    let http = fixture(
        labels(&["keep"]),
        ResponseTemplate::new(200).set_body_json(labels(&["keep", " leading 日本語 "])),
    )
    .await;
    write(
        &http,
        "POST",
        Some(json!({"labels":[" leading 日本語 "]})),
        ResponseTemplate::new(200).set_body_json(labels(&[" leading 日本語 "])),
    )
    .await;
    let client = connect(&http).await;
    let result=client.call_tool(CallToolRequestParams::new("add_issue_labels").with_arguments(json!({"owner":" owner ","repo":" repo ","issue_number":7,"labels":[" leading 日本語 "]}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.structured_content.unwrap(),
        json!({"issue_number":7,"labels":labels(&["keep"," leading 日本語 "])})
    );
    client.cancel().await.unwrap();
}
