mod common;
use common::{web_identity, TestServer};
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::operation::{FieldStatus, OperationOutcome, OutcomeStatus},
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

const RAW: &str = "  日本語 **Markdown**  \n\n    code\nlast  \n";
const EMPTY: &str =
    "<span id='label-milestone'><span class='muted small'>No milestone</span></span>";

fn issue(milestone: Option<u64>) -> Value {
    let mut value = json!({"number":7,"title":"Issue","body":RAW,"state":"closed",
        "labels":[{"name":" exact 日本語 "}],"assignees":[{"login":"alice"}],
        "milestone":null});
    if let Some(number) = milestone {
        value["milestone"] = json!({"number":number,"title":"Release","state":"closed"});
    }
    value
}
async fn auth(http: &TestServer) {
    web_identity(&http.mock_server, "", "alice").await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=fake; Path=/"),
        )
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Home</html>"))
        .mount(&http.mock_server)
        .await;
}
async fn reference(http: &TestServer, number: u64, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/milestones/{number}"
        )))
        .respond_with(response)
        .mount(&http.mock_server)
        .await;
}
async fn fixture(
    http: &TestServer,
    before: Value,
    after: ResponseTemplate,
    post: ResponseTemplate,
) {
    auth(http).await;
    reference(
        http,
        9,
        ResponseTemplate::new(200)
            .set_body_json(json!({"number":9,"title":"Release","state":"closed"})),
    )
    .await;
    let attempted = Arc::new(AtomicBool::new(false));
    let sent = attempted.clone();
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/7/milestone"))
        .respond_with(move |_: &wiremock::Request| {
            sent.store(true, Ordering::SeqCst);
            post.clone()
        })
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/7"))
        .respond_with(move |_: &wiremock::Request| {
            if attempted.load(Ordering::SeqCst) {
                after.clone()
            } else {
                ResponseTemplate::new(200).set_body_json(&before)
            }
        })
        .mount(&http.mock_server)
        .await;
}
fn client(http: &TestServer) -> GitBucketClient {
    http.standard_client_with_web_auth("alice", "fake-password")
}
fn outcome(error: GbMcpError) -> OperationOutcome {
    match error {
        GbMcpError::OperationFailed { outcome, .. } => *outcome,
        error => panic!("Expected an operation error, got {error:?}"),
    }
}
async fn writes(http: &TestServer) -> usize {
    http.mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method != "GET" && r.url.path() != "/signin")
        .count()
}

#[tokio::test]
async fn set_preserves_raw_content_and_assignments_with_one_cookie_only_post() {
    let http = TestServer::start().await;
    fixture(
        &http,
        issue(Some(3)),
        ResponseTemplate::new(200).set_body_json(issue(Some(9))),
        ResponseTemplate::new(200).set_body_string("<div>progress</div>"),
    )
    .await;
    let actual = client(&http)
        .set_issue_milestone("owner", "repo", 7, 9)
        .await
        .unwrap();
    assert_eq!(actual.body.as_deref(), Some(RAW));
    assert_eq!(actual.state, "closed");
    assert_eq!(actual.milestone.unwrap().number, 9);
    let requests = http.mock_server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|r| r.url.path() == "/owner/repo/issues/7/milestone")
        .unwrap();
    assert_eq!(post.body, b"milestoneId=9");
    assert!(!post.headers.contains_key("authorization"));
    assert!(post.headers.contains_key("cookie"));
    assert_eq!(post.headers["x-requested-with"], "XMLHttpRequest");
    assert_eq!(writes(&http).await, 1);
}

#[tokio::test]
async fn clear_requires_an_explicit_native_empty_marker_on_issue_and_pr() {
    for is_pr in [false, true] {
        let http = TestServer::start().await;
        let mut before = issue(Some(9));
        let mut after = issue(None);
        // Native unassigned serialization can omit this key entirely.
        after.as_object_mut().unwrap().remove("milestone");
        if is_pr {
            before["pull_request"] = json!({"url":"http://example.test/pulls/7"});
            after["pull_request"] = before["pull_request"].clone();
        }
        fixture(
            &http,
            before,
            ResponseTemplate::new(200).set_body_json(after),
            ResponseTemplate::new(200),
        )
        .await;
        Mock::given(method("GET"))
            .and(path(if is_pr {
                "/owner/repo/pull/7"
            } else {
                "/owner/repo/issues/7"
            }))
            .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let actual = client(&http)
            .clear_issue_milestone("owner", "repo", 7)
            .await
            .unwrap();
        assert!(actual.milestone.is_none());
        assert_eq!(actual.body.as_deref(), Some(RAW));
        assert_eq!(writes(&http).await, 1);
        let requests = http.mock_server.received_requests().await.unwrap();
        assert_eq!(
            requests
                .iter()
                .find(|r| r.method == "POST" && r.url.path().ends_with("/milestone"))
                .unwrap()
                .body,
            b"milestoneId="
        );
    }
}

#[tokio::test]
async fn matching_assignments_and_proven_empty_state_send_no_business_write() {
    for clear in [false, true] {
        let http = TestServer::start().await;
        fixture(
            &http,
            issue(if clear { None } else { Some(9) }),
            ResponseTemplate::new(500),
            ResponseTemplate::new(500),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY))
            .mount(&http.mock_server)
            .await;
        if clear {
            client(&http)
                .clear_issue_milestone("owner", "repo", 7)
                .await
                .unwrap();
        } else {
            // The matching set needs no credentials or Web session.
            http.standard_client()
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap();
            assert!(!http
                .mock_server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.url.path() == "/signin"));
        }
        assert_eq!(writes(&http).await, 0);
    }
}

#[tokio::test]
async fn rest_only_refuses_before_http_even_with_web_credentials() {
    let http = TestServer::start().await;
    let client = client(&http).with_api_profile(ApiProfile::RestOnly);
    assert!(matches!(
        client.set_issue_milestone("owner", "repo", 7, 9).await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert!(matches!(
        client.clear_issue_milestone("owner", "repo", 7).await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn invalid_target_and_milestone_numbers_are_rejected_without_http() {
    let http = TestServer::start().await;
    for number in [0, i32::MAX as u64 + 1, u64::MAX] {
        assert!(matches!(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, number)
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client(&http)
                .clear_issue_milestone("owner", "repo", number)
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn scoped_references_and_complete_parent_are_required_before_signin() {
    for case in [
        "parent404",
        "parent_id",
        "reference404",
        "reference_id",
        "body",
        "labels",
        "assignees",
        "duplicate",
    ] {
        let http = TestServer::start().await;
        let mut before = issue(Some(3));
        if case == "parent_id" {
            before["number"] = json!(8);
        }
        if ["body", "labels", "assignees"].contains(&case) {
            before.as_object_mut().unwrap().remove(case);
        }
        if case == "duplicate" {
            before["assignees"] = json!([{"login":"alice"},{"login":"alice"}]);
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/issues/7"))
            .respond_with(if case == "parent404" {
                ResponseTemplate::new(404)
            } else {
                ResponseTemplate::new(200).set_body_json(before)
            })
            .mount(&http.mock_server)
            .await;
        reference(&http, 9, if case == "reference404" { ResponseTemplate::new(404) } else { ResponseTemplate::new(200).set_body_json(json!({"number":if case == "reference_id" { 8 } else { 9 },"title":"Release","state":"open"})) }).await;
        assert!(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .is_err(),
            "{case}"
        );
        assert_eq!(writes(&http).await, 0, "{case}");
        assert!(!http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/signin"));
    }
}

#[tokio::test]
async fn post_write_read_failure_keeps_target_and_uncertain_fields_without_retry() {
    for status in [401, 403, 404, 500] {
        let http = TestServer::start().await;
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(status),
            ResponseTemplate::new(200),
        )
        .await;
        let result = outcome(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap_err(),
        );
        assert_eq!(result.status, OutcomeStatus::Unknown);
        assert_eq!(result.target.unwrap().number, Some(7));
        assert!(result
            .fields
            .iter()
            .all(|f| f.status == FieldStatus::Unknown));
        assert_eq!(writes(&http).await, 1);
    }
}

#[tokio::test]
async fn unexpected_post_write_identity_keeps_both_numbers() {
    let http = TestServer::start().await;
    let mut after = issue(Some(9));
    after["number"] = json!(8);
    fixture(
        &http,
        issue(Some(3)),
        ResponseTemplate::new(200).set_body_json(after),
        ResponseTemplate::new(200),
    )
    .await;
    let result = outcome(
        client(&http)
            .set_issue_milestone("owner", "repo", 7, 9)
            .await
            .unwrap_err(),
    );
    assert_eq!(result.status, OutcomeStatus::Unknown);
    let target = result.target.unwrap();
    assert_eq!(target.number, Some(7));
    assert_eq!(target.observed_number, Some(8));
    assert_eq!(writes(&http).await, 1);
}

#[tokio::test]
async fn ignored_assignment_and_changed_preserved_fields_are_not_success() {
    for field in ["milestone", "title", "body", "state", "labels", "assignees"] {
        let http = TestServer::start().await;
        let mut after = issue(Some(9));
        after[field] = match field {
            "milestone" => issue(Some(3))["milestone"].clone(),
            "labels" | "assignees" => json!([]),
            "state" => json!("open"),
            _ => json!("changed"),
        };
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(200).set_body_json(after),
            ResponseTemplate::new(200),
        )
        .await;
        let result = outcome(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            result.status,
            if field == "milestone" {
                OutcomeStatus::NotVerified
            } else {
                OutcomeStatus::Partial
            }
        );
        assert_eq!(
            result
                .fields
                .iter()
                .find(|f| f.field == field)
                .unwrap()
                .status,
            FieldStatus::Mismatched
        );
        assert_eq!(writes(&http).await, 1);
    }
}

#[tokio::test]
async fn incomplete_observations_remain_unknown_or_partial() {
    for field in ["milestone", "body", "labels", "assignees"] {
        let http = TestServer::start().await;
        let mut after = issue(Some(9));
        after.as_object_mut().unwrap().remove(field);
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(200).set_body_json(after),
            ResponseTemplate::new(200),
        )
        .await;
        let result = outcome(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            result.status,
            if field == "milestone" {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::Partial
            }
        );
        assert_eq!(
            result
                .fields
                .iter()
                .find(|f| f.field == field)
                .unwrap()
                .status,
            FieldStatus::Unknown
        );
        assert_eq!(writes(&http).await, 1);
    }
}

#[tokio::test]
async fn verified_clear_is_retained_when_a_preserved_field_mismatches() {
    let http = TestServer::start().await;
    let mut after = issue(None);
    after["body"] = json!("concurrent edit");
    fixture(
        &http,
        issue(Some(9)),
        ResponseTemplate::new(200).set_body_json(after),
        ResponseTemplate::new(200),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let result = outcome(
        client(&http)
            .clear_issue_milestone("owner", "repo", 7)
            .await
            .unwrap_err(),
    );
    assert_eq!(result.status, OutcomeStatus::Partial);
    assert_eq!(
        result
            .fields
            .iter()
            .find(|f| f.field == "milestone")
            .unwrap()
            .status,
        FieldStatus::Verified
    );
    assert_eq!(
        result
            .fields
            .iter()
            .find(|f| f.field == "body")
            .unwrap()
            .status,
        FieldStatus::Mismatched
    );
    assert_eq!(writes(&http).await, 1);
}

#[tokio::test]
async fn clear_does_not_treat_missing_rest_or_missing_ambiguous_html_as_clear() {
    for html in ["<html></html>".to_owned(), EMPTY.repeat(2), "<span id='label-milestone'></span>".into(),
        "<span id='label-milestone'><a href='/owner/repo/issues?milestone=Release'>Release</a></span>".into(),
        "<form action='/signin'><input type='password'></form>".into()] {
        let http = TestServer::start().await;
        fixture(&http, issue(Some(9)), ResponseTemplate::new(200).set_body_json(issue(None)), ResponseTemplate::new(200)).await;
        Mock::given(method("GET")).and(path("/owner/repo/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html)).mount(&http.mock_server).await;
        let result = outcome(client(&http).clear_issue_milestone("owner", "repo", 7).await.unwrap_err());
        assert!(matches!(result.status, OutcomeStatus::Unknown | OutcomeStatus::NotVerified));
        assert_eq!(result.target.unwrap().number, Some(7));
        assert_eq!(writes(&http).await, 1);
    }
}

#[tokio::test]
async fn only_authenticator_rejections_are_definite_and_no_redirect_is_followed() {
    for status in [401, 403, 404, 408, 500, 302, 307, 308] {
        let http = TestServer::start().await;
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(200).set_body_json(issue(Some(9))),
            ResponseTemplate::new(status).insert_header("Location", "/must-not-follow"),
        )
        .await;
        let result = outcome(
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            result.status,
            if [401, 403].contains(&status) {
                OutcomeStatus::NotApplied
            } else {
                OutcomeStatus::Unknown
            },
            "{status}"
        );
        assert_eq!(writes(&http).await, 1);
        assert!(!http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/must-not-follow"));
    }
}

#[tokio::test]
async fn credentials_are_checked_before_the_assignment_attempt() {
    for username in [None, Some("different-account")] {
        let http = TestServer::start().await;
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(500),
            ResponseTemplate::new(200),
        )
        .await;
        let client = username
            .map(|u| http.standard_client_with_web_auth(u, "fake-password"))
            .unwrap_or_else(|| http.standard_client());
        assert!(matches!(
            client.set_issue_milestone("owner", "repo", 7, 9).await,
            Err(GbMcpError::Authentication(_))
        ));
        assert_eq!(writes(&http).await, 0);
    }
}

#[tokio::test]
async fn expired_cookie_before_set_or_clear_stops_the_business_write() {
    for clear in [false, true] {
        let http = TestServer::start().await;
        fixture(
            &http,
            issue(Some(3)),
            ResponseTemplate::new(500),
            ResponseTemplate::new(200),
        )
        .await;
        let cookie_reads = std::sync::atomic::AtomicUsize::new(0);
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(move |request: &wiremock::Request| {
                if request.headers.contains_key("authorization")
                    || cookie_reads.fetch_add(1, Ordering::SeqCst) == 0
                {
                    ResponseTemplate::new(200).set_body_json(json!({"login":"alice"}))
                } else {
                    ResponseTemplate::new(401)
                }
            })
            .with_priority(1)
            .mount(&http.mock_server)
            .await;
        let error = if clear {
            client(&http)
                .clear_issue_milestone("owner", "repo", 7)
                .await
                .unwrap_err()
        } else {
            client(&http)
                .set_issue_milestone("owner", "repo", 7, 9)
                .await
                .unwrap_err()
        };
        assert!(matches!(error, GbMcpError::Authentication(_)));
        assert_eq!(writes(&http).await, 0);
    }
}

#[tokio::test]
async fn mcp_deadline_after_post_keeps_assignment_target_and_unknown_status() {
    let http = TestServer::start().await;
    fixture(
        &http,
        issue(Some(3)),
        ResponseTemplate::new(200)
            .set_body_json(issue(Some(9)))
            .set_delay(std::time::Duration::from_secs(2)),
        ResponseTemplate::new(200),
    )
    .await;
    let server = GitBucketMcpServer::with_api_and_timeout(
        Arc::new(client(&http)),
        std::time::Duration::from_millis(500),
    )
    .unwrap();
    let (server_transport, client_transport) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let running = server.serve(server_transport).await.unwrap();
        let _ = running.waiting().await;
    });
    let mcp = ().serve(client_transport).await.unwrap();
    let result = mcp
        .call_tool(
            CallToolRequestParams::new("set_issue_milestone").with_arguments(
                json!({"owner":"owner","repo":"repo","issue_number":7,"milestone_number":9})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let data = result.structured_content.unwrap();
    assert_eq!(data["operation"]["target"]["number"], 7);
    assert_eq!(data["operation"]["status"], "unknown");
    assert_eq!(data["cause"]["kind"], "timeout_error");
    assert_eq!(writes(&http).await, 1);
    mcp.cancel().await.unwrap();
}
