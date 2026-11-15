mod common;
use common::{web_identity, TestServer};
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::operation::{FieldStatus, OperationOutcome, OutcomeStatus},
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

const RAW: &str = "  日本語 **Markdown**  \n\n    code\nlast  \n";
fn pr(draft: bool) -> Value {
    json!({"number":7,"title":"PR","body":RAW,"state":"open","draft":draft,"merged":false,"mergeable":null,
        "labels":[{"name":" exact 日本語 "}],"assignees":[{"login":"alice"}],
        "head":{"ref":"feature/ready","sha":"b".repeat(40),"repo":{"name":"fork","full_name":"contributor/fork","private":true}},
        "base":{"ref":"main","sha":"a".repeat(40),"repo":{"name":"repo","full_name":"owner/repo","private":true}}})
}
struct Fixture {
    http: TestServer,
    posts: Arc<AtomicUsize>,
}
impl Fixture {
    fn client(&self) -> GitBucketClient {
        self.http
            .standard_client_with_web_auth("alice", "fake-password")
    }
    fn posts(&self) -> usize {
        self.posts.load(Ordering::SeqCst)
    }
}
async fn fixture(
    before: Value,
    preflight: Option<Value>,
    after: ResponseTemplate,
    post: ResponseTemplate,
) -> Fixture {
    let http = TestServer::start().await;
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
    let posts = Arc::new(AtomicUsize::new(0));
    let attempted = posts.clone();
    Mock::given(method("POST"))
        .and(path("/owner/repo/pull/7/update_draft"))
        .respond_with(move |r: &wiremock::Request| {
            assert!(r.body.is_empty());
            assert!(!r.headers.contains_key("authorization"));
            assert!(r.headers.contains_key("cookie"));
            assert_eq!(r.headers["x-requested-with"], "XMLHttpRequest");
            attempted.fetch_add(1, Ordering::SeqCst);
            post.clone()
        })
        .mount(&http.mock_server)
        .await;
    let attempted = posts.clone();
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(move |_: &wiremock::Request| {
            if attempted.load(Ordering::SeqCst) != 0 {
                return after.clone();
            }
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(&before)
            } else {
                ResponseTemplate::new(200).set_body_json(preflight.as_ref().unwrap_or(&before))
            }
        })
        .mount(&http.mock_server)
        .await;
    Fixture { http, posts }
}
fn outcome(error: GbMcpError) -> OperationOutcome {
    match error {
        GbMcpError::OperationFailed { outcome, .. } => *outcome,
        error => panic!("Expected operation evidence: {error:?}"),
    }
}
fn field(o: &OperationOutcome, name: &str) -> FieldStatus {
    o.fields.iter().find(|f| f.field == name).unwrap().status
}
async fn success_fixture() -> Fixture {
    fixture(
        pr(true),
        None,
        ResponseTemplate::new(200).set_body_json(pr(false)),
        ResponseTemplate::new(200),
    )
    .await
}

#[tokio::test]
async fn ready_is_one_cookie_only_empty_post_with_independent_false_and_preservation_checks() {
    let f = success_fixture().await;
    let actual = f
        .client()
        .mark_pull_request_ready("owner", "repo", 7)
        .await
        .unwrap();
    assert_eq!(actual.draft, Some(false));
    assert_eq!(actual.merged, Some(false));
    assert_eq!(actual.body.as_deref(), Some(RAW));
    assert_eq!(actual.mergeable, None);
    assert_eq!(actual.head.unwrap().sha, Some("b".repeat(40)));
    assert_eq!(actual.labels.unwrap()[0].name, " exact 日本語 ");
    assert_eq!(actual.assignees.unwrap()[0].login, "alice");
    assert_eq!(f.posts(), 1);
    let requests = f.http.mock_server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path().ends_with("/pulls/7"))
            .count(),
        3
    );
    assert!(!requests
        .iter()
        .any(|r| r.method == "PATCH" || r.method == "PUT" || r.url.path().ends_with("/merge")));
}

#[tokio::test]
async fn already_ready_is_a_standard_profile_pat_only_noop() {
    let f = fixture(
        pr(false),
        None,
        ResponseTemplate::new(500),
        ResponseTemplate::new(500),
    )
    .await;
    let actual = f
        .http
        .standard_client()
        .mark_pull_request_ready("owner", "repo", 7)
        .await
        .unwrap();
    assert_eq!(actual.draft, Some(false));
    assert_eq!(f.posts(), 0);
    assert!(!f
        .http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/signin"));
}

#[tokio::test]
async fn rest_only_and_invalid_numbers_are_rejected_before_all_http() {
    let http = TestServer::start().await;
    let c = http.standard_client_with_web_auth("alice", "fake-password");
    for n in [0, i32::MAX as u64 + 1, u64::MAX] {
        assert!(matches!(
            c.mark_pull_request_ready("owner", "repo", n).await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    let c = c.with_api_profile(ApiProfile::RestOnly);
    assert!(matches!(
        c.mark_pull_request_ready("owner", "repo", 7).await,
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
async fn missing_pr_empty_or_null_response_stops_before_signin_or_ready_post() {
    for body in ["", "null"] {
        let http = TestServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/pulls/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let result = http
            .standard_client_with_web_auth("alice", "fake-password")
            .mark_pull_request_ready("owner", "repo", 7)
            .await;
        assert!(matches!(
            result,
            Err(GbMcpError::Other(_) | GbMcpError::Json(_))
        ));
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn closed_merged_unknown_or_incomplete_baselines_fail_before_signin() {
    for case in [
        "closed",
        "merged",
        "merged_unknown",
        "draft_unknown",
        "draft_missing",
        "number",
        "body",
        "head",
        "base",
        "head_sha",
        "base_repository",
        "labels",
        "assignees",
        "duplicate",
    ] {
        let mut before = pr(true);
        match case {
            "closed" => before["state"] = json!("closed"),
            "merged" => before["merged"] = json!(true),
            "merged_unknown" => before["merged"] = Value::Null,
            "draft_unknown" => before["draft"] = Value::Null,
            "draft_missing" => {
                before.as_object_mut().unwrap().remove("draft");
            }
            "number" => before["number"] = json!(8),
            "head_sha" => before["head"]["sha"] = json!("B".repeat(40)),
            "base_repository" => before["base"]["repo"]["full_name"] = json!("other/repo"),
            "duplicate" => before["assignees"] = json!([{"login":"alice"},{"login":"alice"}]),
            key => {
                before.as_object_mut().unwrap().remove(key);
            }
        }
        let f = fixture(
            before,
            None,
            ResponseTemplate::new(500),
            ResponseTemplate::new(200),
        )
        .await;
        assert!(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .is_err(),
            "{case}"
        );
        assert_eq!(f.posts(), 0);
        assert!(!f
            .http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/signin"));
    }
}

#[tokio::test]
async fn final_prewrite_read_stops_on_content_revision_scope_and_state_changes() {
    for case in [
        "title",
        "body",
        "head",
        "base",
        "labels",
        "assignees",
        "state",
        "merged",
        "number",
        "draft_unknown",
    ] {
        let mut current = pr(true);
        match case {
            "title" | "body" => current[case] = json!("new content"),
            "head" => current["head"]["sha"] = json!("c".repeat(40)),
            "base" => current["base"]["ref"] = json!("changed"),
            "labels" | "assignees" => current[case] = json!([]),
            "state" => current["state"] = json!("closed"),
            "merged" => current["merged"] = json!(true),
            "number" => current["number"] = json!(8),
            _ => current["draft"] = Value::Null,
        }
        let f = fixture(
            pr(true),
            Some(current),
            ResponseTemplate::new(500),
            ResponseTemplate::new(200),
        )
        .await;
        assert!(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .is_err(),
            "{case}"
        );
        assert_eq!(f.posts(), 0);
    }
}

#[tokio::test]
async fn becoming_ready_during_signin_returns_a_verified_noop_without_post() {
    let f = fixture(
        pr(true),
        Some(pr(false)),
        ResponseTemplate::new(500),
        ResponseTemplate::new(500),
    )
    .await;
    assert_eq!(
        f.client()
            .mark_pull_request_ready("owner", "repo", 7)
            .await
            .unwrap()
            .draft,
        Some(false)
    );
    assert_eq!(f.posts(), 0);
}

#[tokio::test]
async fn missing_credentials_identity_mismatch_or_expiry_stop_the_first_business_write() {
    let f = success_fixture().await;
    assert!(matches!(
        f.http
            .standard_client()
            .mark_pull_request_ready("owner", "repo", 7)
            .await,
        Err(GbMcpError::Authentication(_))
    ));
    assert!(matches!(
        f.http
            .standard_client_with_web_auth("different", "fake-password")
            .mark_pull_request_ready("owner", "repo", 7)
            .await,
        Err(GbMcpError::Authentication(_))
    ));
    let cookies = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(move |r: &wiremock::Request| {
            if r.headers.contains_key("authorization")
                || cookies.fetch_add(1, Ordering::SeqCst) == 0
            {
                ResponseTemplate::new(200).set_body_json(json!({"login":"alice"}))
            } else {
                ResponseTemplate::new(401)
            }
        })
        .with_priority(1)
        .mount(&f.http.mock_server)
        .await;
    assert!(matches!(
        f.client().mark_pull_request_ready("owner", "repo", 7).await,
        Err(GbMcpError::Authentication(_))
    ));
    assert_eq!(f.posts(), 0);
}

#[tokio::test]
async fn native_auth_and_missing_or_head_permission_rejections_are_definite() {
    for code in [401, 403, 404] {
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200).set_body_json(pr(false)),
            ResponseTemplate::new(code),
        )
        .await;
        let o = outcome(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .unwrap_err(),
        );
        assert_eq!(o.status, OutcomeStatus::NotApplied);
        assert_eq!(o.target.unwrap().number, Some(7));
        assert_eq!(f.posts(), 1);
    }
}

#[tokio::test]
async fn other_write_failures_and_redirects_stay_unknown_without_follow_or_replay() {
    for code in [400, 408, 422, 500, 302, 303, 307, 308] {
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200).set_body_json(pr(false)),
            ResponseTemplate::new(code).insert_header("Location", "/must-not-follow"),
        )
        .await;
        let o = outcome(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .unwrap_err(),
        );
        assert_eq!(o.status, OutcomeStatus::Unknown);
        assert_eq!(f.posts(), 1);
        assert!(!f
            .http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/must-not-follow"));
    }
    for html in [
        "<form action='/signin'><input type='password' value='fake-secret'></form>",
        "<span id='error-ready'>fake-secret</span>",
    ] {
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200).set_body_json(pr(false)),
            ResponseTemplate::new(200).set_body_string(html),
        )
        .await;
        let error = f
            .client()
            .mark_pull_request_ready("owner", "repo", 7)
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("fake-secret"));
        assert_eq!(outcome(error).status, OutcomeStatus::Unknown);
    }
}

#[tokio::test]
async fn post_write_read_errors_remain_unknown_even_for_404_and_keep_initial_head() {
    for code in [401, 403, 404, 500] {
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(code),
            ResponseTemplate::new(200),
        )
        .await;
        let o = outcome(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .unwrap_err(),
        );
        assert_eq!(o.status, OutcomeStatus::Unknown);
        assert!(o.fields.iter().all(|f| f.status == FieldStatus::Unknown));
        let target = o.target.unwrap();
        assert_eq!(target.number, Some(7));
        assert_eq!(target.commit, Some("b".repeat(40)));
        assert_eq!(f.posts(), 1);
    }
}

#[tokio::test]
async fn ignored_transition_and_unknown_draft_are_not_verified_as_ready() {
    for observed in [json!(true), Value::Null] {
        let mut after = pr(false);
        after["draft"] = observed.clone();
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200).set_body_json(after),
            ResponseTemplate::new(200),
        )
        .await;
        let o = outcome(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            o.status,
            if observed.is_null() {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::NotVerified
            }
        );
        assert_eq!(
            field(&o, "draft"),
            if observed.is_null() {
                FieldStatus::Unknown
            } else {
                FieldStatus::Mismatched
            }
        );
        assert_eq!(f.posts(), 1);
    }
}

#[tokio::test]
async fn observed_ready_is_retained_alongside_mismatched_or_missing_preserved_fields() {
    for name in [
        "title",
        "body",
        "state",
        "merged",
        "head",
        "base",
        "labels",
        "assignees",
    ] {
        for missing in [false, true] {
            if missing && ["title", "state"].contains(&name) {
                continue;
            }
            let mut after = pr(false);
            if missing {
                after.as_object_mut().unwrap().remove(name);
            } else {
                match name {
                    "title" | "body" => after[name] = json!("changed"),
                    "state" => after[name] = json!("closed"),
                    "merged" => after[name] = json!(true),
                    "head" => after["head"]["sha"] = json!("c".repeat(40)),
                    "base" => after["base"]["ref"] = json!("other"),
                    _ => after[name] = json!([]),
                }
            }
            let f = fixture(
                pr(true),
                None,
                ResponseTemplate::new(200).set_body_json(after),
                ResponseTemplate::new(200),
            )
            .await;
            let o = outcome(
                f.client()
                    .mark_pull_request_ready("owner", "repo", 7)
                    .await
                    .unwrap_err(),
            );
            assert_eq!(o.status, OutcomeStatus::Partial, "{name} missing={missing}");
            assert_eq!(field(&o, "draft"), FieldStatus::Verified);
            assert_eq!(
                field(&o, name),
                if missing {
                    FieldStatus::Unknown
                } else {
                    FieldStatus::Mismatched
                }
            );
            assert_eq!(f.posts(), 1);
        }
    }
}

#[tokio::test]
async fn unexpected_response_identity_keeps_intended_and_observed_pr_numbers() {
    for number in [0, 8] {
        let mut after = pr(false);
        after["number"] = json!(number);
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200).set_body_json(after),
            ResponseTemplate::new(200),
        )
        .await;
        let o = outcome(
            f.client()
                .mark_pull_request_ready("owner", "repo", 7)
                .await
                .unwrap_err(),
        );
        assert_eq!(o.status, OutcomeStatus::Unknown);
        let target = o.target.unwrap();
        assert_eq!(target.number, Some(7));
        assert_eq!(target.observed_number, Some(number));
        assert_eq!(target.commit, Some("b".repeat(40)));
    }
}

async fn mcp(server: GitBucketMcpServer) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_io, client_io) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.unwrap()
}

#[tokio::test]
async fn mcp_normalizes_identifiers_and_rejects_reverse_flags_and_readonly_calls() {
    let f = success_fixture().await;
    let client = mcp(GitBucketMcpServer::new(f.client())).await;
    let args = json!({"owner":" owner ","repo":" repo ","pull_number":7});
    for flag in ["draft", "ready", "state", "expected_head_sha"] {
        let mut bad = args.clone();
        bad[flag] = Value::Null;
        let error = client
            .call_tool(
                CallToolRequestParams::new("mark_pull_request_ready")
                    .with_arguments(bad.as_object().unwrap().clone()),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, rmcp::service::ServiceError::McpError(ref error)
            if error.code == rmcp::model::ErrorCode::INVALID_PARAMS)
        );
    }
    assert!(f
        .http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    let result = client
        .call_tool(
            CallToolRequestParams::new("mark_pull_request_ready")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["draft"], false);
    client.cancel().await.unwrap();
    let readonly =
        mcp(GitBucketMcpServer::new(f.client()).with_access_mode(AccessMode::ReadOnly)).await;
    let result = readonly
        .call_tool(
            CallToolRequestParams::new("mark_pull_request_ready")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "read_only_error"
    );
    assert_eq!(f.posts(), 1);
    readonly.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_deadline_or_cancellation_after_post_keeps_target_head_and_unknown_draft() {
    for cancel in [false, true] {
        let f = fixture(
            pr(true),
            None,
            ResponseTemplate::new(200)
                .set_body_json(pr(false))
                .set_delay(Duration::from_secs(2)),
            ResponseTemplate::new(200),
        )
        .await;
        let server = GitBucketMcpServer::with_api_and_timeout(
            Arc::new(f.client()),
            if cancel {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(500)
            },
        )
        .unwrap();
        let served = server.clone();
        let (server_io, client_io) = tokio::io::duplex(8192);
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let running = served.serve(server_io).await.unwrap();
            let _ = tx.send(running.peer().clone());
            let _ = running.waiting().await;
        });
        let peer = ().serve(client_io).await.unwrap();
        let context = rmcp::service::RequestContext::new(
            rmcp::model::RequestId::Number(123),
            rx.await.unwrap(),
        );
        let cancellation = context.ct.clone();
        let args = json!({"owner":"owner","repo":"repo","pull_number":7});
        let task = tokio::spawn(async move {
            server
                .call_tool(
                    CallToolRequestParams::new("mark_pull_request_ready")
                        .with_arguments(args.as_object().unwrap().clone()),
                    context,
                )
                .await
                .unwrap()
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while f.posts() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        if cancel {
            cancellation.cancel();
        }
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        let data = result.structured_content.unwrap();
        assert_eq!(data["operation"]["status"], "unknown");
        assert_eq!(data["operation"]["target"]["number"], 7);
        assert_eq!(data["operation"]["target"]["commit"], "b".repeat(40));
        assert_eq!(
            data["cause"]["kind"],
            if cancel {
                "cancelled_error"
            } else {
                "timeout_error"
            }
        );
        assert_eq!(f.posts(), 1);
        peer.cancel().await.unwrap();
    }
}
