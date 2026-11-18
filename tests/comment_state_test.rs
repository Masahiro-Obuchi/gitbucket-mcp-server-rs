mod common;

use common::TestServer;
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::{
        comment::CreateComment,
        operation::{FieldStatus, OperationOutcome, OutcomeStatus},
    },
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, service::RequestContext, ServerHandler, ServiceExt};
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

const ISSUE: &str = "/api/v3/repos/owner/repo/issues/2";
const COMMENTS: &str = "/api/v3/repos/owner/repo/issues/2/comments";
const RAW: &str = "  日本語 **Markdown**  \n\n    code\nlast  \n";
fn issue(state: &str) -> Value {
    json!({"number":2,"title":"Parent","body":"  parent raw  \n","state":state,"labels":[{"name":" exact label "}],"assignees":[{"login":"owner"}],"milestone":{"number":3,"title":"M","state":"open"}})
}
fn pr(state: &str) -> Value {
    json!({"number":2,"title":"Parent","state":state,"merged":false,"draft":true,"head":{"ref":"feature","sha":"b".repeat(40),"repo":{"name":"fork","full_name":"author/fork","private":true}},"base":{"ref":"main","sha":"a".repeat(40),"repo":{"name":"repo","full_name":"owner/repo","private":true}}})
}
fn comment() -> Value {
    json!({"id":7,"body":RAW,"user":{"login":"actor"},"created_at":"2026-01-01T00:00:00Z","html_url":"https://example.test/arbitrary-link"})
}
fn ok(value: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(value)
}
struct Fixture {
    http: TestServer,
}
impl Fixture {
    fn client(&self) -> GitBucketClient {
        self.http.client()
    }
    async fn posts(&self) -> usize {
        self.http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .count()
    }
    async fn call(
        &self,
        close: bool,
    ) -> gitbucket_mcp_server::error::Result<
        gitbucket_mcp_server::models::comment::CommentStateChange,
    > {
        invoke(&self.client(), close).await
    }
}
async fn invoke(
    client: &GitBucketClient,
    close: bool,
) -> gitbucket_mcp_server::error::Result<gitbucket_mcp_server::models::comment::CommentStateChange>
{
    let body = CreateComment { body: RAW.into() };
    if close {
        client
            .close_issue_with_comment("owner", "repo", 2, &body)
            .await
    } else {
        client
            .reopen_issue_with_comment("owner", "repo", 2, &body)
            .await
    }
}
async fn fixture(
    before: Value,
    after: ResponseTemplate,
    post: ResponseTemplate,
    comments: ResponseTemplate,
) -> Fixture {
    let http = TestServer::start().await;
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path(ISSUE))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ok(before.clone())
            } else {
                after.clone()
            }
        })
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ok(json!({"login":"actor"})))
        .mount(&http.mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path(COMMENTS))
        .respond_with(post)
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMMENTS))
        .respond_with(comments)
        .mount(&http.mock_server)
        .await;
    Fixture { http }
}
async fn pr_reads(f: &Fixture, before: Value, after: ResponseTemplate) {
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/2"))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ok(before.clone())
            } else {
                after.clone()
            }
        })
        .mount(&f.http.mock_server)
        .await;
}
fn outcome(error: &GbMcpError) -> &OperationOutcome {
    match error {
        GbMcpError::OperationFailed { outcome, .. } => outcome,
        e => panic!("expected operation evidence: {e}"),
    }
}
fn field(outcome: &OperationOutcome, name: &str) -> FieldStatus {
    outcome
        .fields
        .iter()
        .find(|f| f.field == name)
        .unwrap()
        .status
}

#[tokio::test]
async fn close_and_reopen_use_one_action_query_post_and_return_independent_models_in_both_profiles()
{
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        for close in [true, false] {
            let (before, after) = if close {
                ("open", "closed")
            } else {
                ("closed", "open")
            };
            let f = fixture(
                issue(before),
                ok(issue(after)),
                ok(json!({"id":7,"body":123,"user":{}})),
                ok(json!([comment()])),
            )
            .await;
            let value = invoke(&f.client().with_api_profile(profile), close)
                .await
                .unwrap();
            assert_eq!(serde_json::to_value(value.comment).unwrap()["body"], RAW);
            assert_eq!(value.issue.state, after);
            assert!(value.pull_request.is_none());
            let requests = f.http.mock_server.received_requests().await.unwrap();
            let posts: Vec<_> = requests.iter().filter(|r| r.method == "POST").collect();
            assert_eq!(posts.len(), 1);
            assert_eq!(posts[0].body_json::<Value>().unwrap(), json!({"body":RAW}));
            assert_eq!(
                posts[0].url.query(),
                Some(if close {
                    "action=close"
                } else {
                    "action=reopen"
                })
            );
            assert!(requests
                .iter()
                .all(|r| r.headers["authorization"] == "token test-token"
                    && !r.headers.contains_key("cookie")));
            assert_eq!(requests.len(), 5);
            assert!(requests
                .iter()
                .all(|r| r.method == "GET" || r.method == "POST"));
        }
    }
}

#[tokio::test]
async fn pr_actions_preserve_draft_merged_and_complete_fork_revisions() {
    for close in [true, false] {
        for draft in [true, false] {
            let (before, after) = if close {
                ("open", "closed")
            } else {
                ("closed", "open")
            };
            let mut b = issue(before);
            b["pull_request"] = json!({});
            let mut a = b.clone();
            a["state"] = json!(after);
            let f = fixture(b, ok(a), ok(comment()), ok(json!([comment()]))).await;
            let mut p = pr(before);
            p["draft"] = json!(draft);
            let mut q = p.clone();
            q["state"] = json!(after);
            pr_reads(&f, p, ok(q)).await;
            let result = f.call(close).await.unwrap();
            let result = result.pull_request.unwrap();
            assert_eq!(result.draft, Some(draft));
            assert_eq!(result.merged, Some(false));
            assert_eq!(result.state, after);
            assert_eq!(f.posts().await, 1);
        }
    }
}

#[tokio::test]
async fn invalid_ids_blank_bodies_and_identifiers_send_no_http_even_for_direct_clients() {
    let http = TestServer::start().await;
    for number in [0, 2147483648, u64::MAX] {
        assert!(matches!(
            http.client()
                .close_issue_with_comment(
                    "owner",
                    "repo",
                    number,
                    &CreateComment { body: RAW.into() }
                )
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    for body in ["", "  \n\t", "\u{2003}"] {
        assert!(matches!(
            http.client()
                .reopen_issue_with_comment("owner", "repo", 2, &CreateComment { body: body.into() })
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert!(http
        .client()
        .close_issue_with_comment("../owner", "repo", 2, &CreateComment { body: RAW.into() })
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
async fn missing_or_foreign_parent_and_incomplete_baselines_stop_before_comment_creation() {
    let mut cases = vec![];
    for (key, value) in [
        ("number", json!(8)),
        ("body", Value::Null),
        ("labels", Value::Null),
        ("assignees", Value::Null),
        ("state", json!("unknown")),
        ("labels", json!([{"name":"x"},{"name":"x"}])),
        ("assignees", json!([{"login":"a"},{"login":"a"}])),
    ] {
        let mut v = issue("open");
        v[key] = value;
        cases.push(v);
    }
    for before in cases {
        let f = fixture(
            before,
            ok(issue("closed")),
            ok(comment()),
            ok(json!([comment()])),
        )
        .await;
        assert!(f.call(true).await.is_err());
        assert_eq!(f.posts().await, 0);
        assert_eq!(
            f.http.mock_server.received_requests().await.unwrap().len(),
            1
        );
    }
    for status in [401, 403, 404, 500] {
        let http = TestServer::start().await;
        Mock::given(method("GET"))
            .and(path(ISSUE))
            .respond_with(ResponseTemplate::new(status))
            .mount(&http.mock_server)
            .await;
        assert!(
            matches!(invoke(&http.client(),true).await,Err(GbMcpError::Api { status:s,.. }) if s==status)
        );
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn repeated_or_already_requested_states_do_not_append_another_comment() {
    for close in [true, false] {
        let f = fixture(
            issue(if close { "closed" } else { "open" }),
            ok(issue("open")),
            ok(comment()),
            ok(json!([comment()])),
        )
        .await;
        assert!(matches!(
            f.call(close).await,
            Err(GbMcpError::Unsupported(_))
        ));
        assert_eq!(f.posts().await, 0);
    }
}

#[tokio::test]
async fn merged_unknown_or_incomplete_prs_stop_before_post() {
    for (key, value) in [
        ("merged", json!(true)),
        ("merged", Value::Null),
        ("draft", Value::Null),
        ("head", Value::Null),
        ("base", Value::Null),
        ("number", json!(8)),
        ("state", json!("open")),
    ] {
        let mut b = issue("closed");
        b["pull_request"] = json!({});
        let f = fixture(b, ok(issue("open")), ok(comment()), ok(json!([comment()]))).await;
        let mut p = pr("closed");
        p[key] = value;
        pr_reads(&f, p, ok(pr("open"))).await;
        assert!(matches!(
            f.call(false).await,
            Err(GbMcpError::Unsupported(_))
        ));
        assert_eq!(f.posts().await, 0);
    }
}

#[tokio::test]
async fn ignored_state_action_keeps_verified_comment_and_reports_partial() {
    let f = fixture(
        issue("open"),
        ok(issue("open")),
        ok(comment()),
        ok(json!([comment()])),
    )
    .await;
    let error = f.call(true).await.unwrap_err();
    let o = outcome(&error);
    assert_eq!(o.status, OutcomeStatus::Partial);
    assert_eq!(field(o, "state"), FieldStatus::Mismatched);
    assert_eq!(field(o, "comment.body"), FieldStatus::Verified);
    assert_eq!(field(o, "comment.parent"), FieldStatus::Verified);
    assert_eq!(o.target.as_ref().unwrap().number, Some(7));
    assert_eq!(o.target.as_ref().unwrap().parent_number, Some(2));
    assert_eq!(f.posts().await, 1);
}

#[tokio::test]
async fn absent_duplicate_missing_or_changed_comment_observations_never_verify_both_effects() {
    let mut missing_body = comment();
    missing_body["body"] = Value::Null;
    let mut missing_author = comment();
    missing_author["user"] = Value::Null;
    let mut wrong_author = comment();
    wrong_author["user"]["login"] = json!("other");
    let mut changed = comment();
    changed["body"] = json!("normalized");
    let mut other = comment();
    other["id"] = json!(8);
    for comments in [
        json!([]),
        json!([other]),
        json!([comment(), comment()]),
        json!([missing_body]),
        json!([missing_author]),
        json!([wrong_author]),
        json!([changed]),
    ] {
        let f = fixture(
            issue("open"),
            ok(issue("closed")),
            ok(comment()),
            ok(comments),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        let o = outcome(&error);
        assert_eq!(o.status, OutcomeStatus::Partial);
        assert_eq!(field(o, "state"), FieldStatus::Verified);
        assert_eq!(o.target.as_ref().unwrap().number, Some(7));
        assert_eq!(f.posts().await, 1);
    }
}

#[tokio::test]
async fn parent_read_errors_do_not_discard_later_verified_comment() {
    for status in [401, 403, 404, 500] {
        let f = fixture(
            issue("open"),
            ResponseTemplate::new(status),
            ok(comment()),
            ok(json!([comment()])),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        let o = outcome(&error);
        assert_eq!(o.status, OutcomeStatus::Partial);
        assert_eq!(field(o, "state"), FieldStatus::Unknown);
        assert_eq!(field(o, "comment.body"), FieldStatus::Verified);
        assert_eq!(f.posts().await, 1);
    }
}

#[tokio::test]
async fn comment_collection_read_errors_retain_verified_state() {
    for status in [401, 403, 404, 500] {
        let f = fixture(
            issue("closed"),
            ok(issue("open")),
            ok(comment()),
            ResponseTemplate::new(status),
        )
        .await;
        let error = f.call(false).await.unwrap_err();
        let o = outcome(&error);
        assert_eq!(o.status, OutcomeStatus::Partial);
        assert_eq!(field(o, "state"), FieldStatus::Verified);
        assert_eq!(field(o, "comment.parent"), FieldStatus::Unknown);
        assert_eq!(f.posts().await, 1);
    }
}

#[tokio::test]
async fn wrong_parent_identity_retains_created_comment_and_records_observed_parent() {
    for number in [0, 8] {
        let mut after = issue("closed");
        after["number"] = json!(number);
        let f = fixture(
            issue("open"),
            ok(after),
            ok(comment()),
            ok(json!([comment()])),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        let o = outcome(&error);
        let target = o.target.as_ref().unwrap();
        assert_eq!(target.number, Some(7));
        assert_eq!(target.parent_number, Some(2));
        assert_eq!(target.observed_parent_number, Some(number));
        assert_eq!(field(o, "state"), FieldStatus::Unknown);
        assert_eq!(field(o, "comment.body"), FieldStatus::Verified);
        assert_eq!(o.status, OutcomeStatus::Partial);
    }
}

#[tokio::test]
async fn retained_field_changes_or_missing_values_are_reported_alongside_saved_effects() {
    for (key, value) in [
        ("title", json!("changed")),
        ("body", Value::Null),
        ("labels", Value::Null),
        ("assignees", json!([])),
        ("milestone", Value::Null),
        ("pull_request", json!({})),
        ("state", json!("unknown")),
    ] {
        let mut after = issue("closed");
        after[key] = value;
        let f = fixture(
            issue("open"),
            ok(after),
            ok(comment()),
            ok(json!([comment()])),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        assert_eq!(outcome(&error).status, OutcomeStatus::Partial);
        assert_eq!(f.posts().await, 1);
    }
}

#[tokio::test]
async fn pr_later_foreign_identity_or_changed_revision_keeps_issue_and_comment_evidence() {
    for (key, value) in [
        ("number", json!(8)),
        ("head", Value::Null),
        ("draft", json!(false)),
        ("merged", json!(true)),
        ("state", json!("closed")),
    ] {
        let mut before = issue("closed");
        before["pull_request"] = json!({});
        let mut after = before.clone();
        after["state"] = json!("open");
        let f = fixture(before, ok(after), ok(comment()), ok(json!([comment()]))).await;
        let mut p = pr("open");
        p[key] = value;
        pr_reads(&f, pr("closed"), ok(p)).await;
        let error = f.call(false).await.unwrap_err();
        let o = outcome(&error);
        assert_eq!(o.status, OutcomeStatus::Partial);
        assert_eq!(field(o, "state"), FieldStatus::Verified);
        assert_eq!(field(o, "comment.body"), FieldStatus::Verified);
        assert_eq!(o.target.as_ref().unwrap().commit, Some("b".repeat(40)));
    }
}

#[tokio::test]
async fn rejected_and_uncertain_post_responses_are_never_retried_or_followed() {
    for status in [400, 401, 403, 404, 408, 422, 500, 302, 307] {
        let f = fixture(
            issue("open"),
            ok(issue("closed")),
            ResponseTemplate::new(status)
                .insert_header("Location", "/unexpected?secret=sentinel")
                .set_body_string("private response sentinel"),
            ok(json!([comment()])),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        let o = outcome(&error);
        assert_eq!(
            o.status,
            if matches!(status, 400 | 401 | 403 | 422) {
                OutcomeStatus::NotApplied
            } else {
                OutcomeStatus::Unknown
            }
        );
        assert_eq!(o.target.as_ref().unwrap().parent_number, Some(2));
        assert_eq!(o.target.as_ref().unwrap().number, None);
        assert_eq!(f.posts().await, 1);
        assert_eq!(
            f.http.mock_server.received_requests().await.unwrap().len(),
            3
        );
        assert!(!format!("{error:?}").contains("sentinel"));
    }
}

#[tokio::test]
async fn invalid_or_unknown_created_comment_id_cannot_turn_acknowledgement_into_success() {
    for response in [
        ok(json!({"id":0})),
        ok(json!({"id":2147483648u64})),
        ok(json!({})),
        ResponseTemplate::new(200).set_body_string(""),
        ok(json!({"status":404,"body":""})),
    ] {
        let f = fixture(
            issue("open"),
            ok(issue("closed")),
            response,
            ok(json!([comment()])),
        )
        .await;
        let error = f.call(true).await.unwrap_err();
        assert_eq!(outcome(&error).status, OutcomeStatus::Unknown);
        assert_eq!(f.posts().await, 1);
    }
}

async fn connect(
    server: GitBucketMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_io, client_io) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.unwrap()
}

#[tokio::test]
async fn mcp_normalizes_identifiers_rejects_extra_or_blank_inputs_and_enforces_readonly() {
    let f = fixture(
        issue("open"),
        ok(issue("closed")),
        ok(comment()),
        ok(json!([comment()])),
    )
    .await;
    let peer = connect(GitBucketMcpServer::new(f.client())).await;
    let args = json!({"owner":" owner ","repo":" repo ","issue_number":2,"body":RAW});
    let result = peer
        .call_tool(
            CallToolRequestParams::new("close_issue_with_comment")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    for extra in [
        json!({"action":"close"}),
        json!({"state":"closed"}),
        json!({"draft":false}),
        json!({"body":" \n"}),
        json!({"issue_number":0}),
    ] {
        let mut invalid = args.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = peer
            .call_tool(
                CallToolRequestParams::new("close_issue_with_comment")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await;
        assert!(result.is_err() || result.unwrap().is_error == Some(true));
    }
    assert_eq!(f.posts().await, 1);
    peer.cancel().await.unwrap();
    let readonly = connect(
        GitBucketMcpServer::new(f.client())
            .with_access_mode(gitbucket_mcp_server::server::AccessMode::ReadOnly),
    )
    .await;
    let result = readonly
        .call_tool(
            CallToolRequestParams::new("reopen_issue_with_comment")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "read_only_error"
    );
    assert_eq!(f.posts().await, 1);
    readonly.cancel().await.unwrap();
}

#[tokio::test]
async fn deadlines_and_cancellation_retain_comment_id_parent_and_verified_state_prefix() {
    for cancel in [false, true] {
        for delay_post in [false, true] {
            let post = if delay_post {
                ok(comment()).set_delay(Duration::from_secs(2))
            } else {
                ok(comment())
            };
            let comments = if delay_post {
                ok(json!([comment()]))
            } else {
                ok(json!([comment()])).set_delay(Duration::from_secs(2))
            };
            let f = fixture(issue("open"), ok(issue("closed")), post, comments).await;
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
            let context =
                RequestContext::new(rmcp::model::RequestId::Number(123), rx.await.unwrap());
            let cancellation = context.ct.clone();
            let task = tokio::spawn(async move {
                server
                    .call_tool(
                        CallToolRequestParams::new("close_issue_with_comment").with_arguments(
                            json!({"owner":"owner","repo":"repo","issue_number":2,"body":RAW})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                        context,
                    )
                    .await
                    .unwrap()
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let requests = f.http.mock_server.received_requests().await.unwrap();
                    if requests.iter().any(|r| {
                        r.url.path() == COMMENTS
                            && r.method == if delay_post { "POST" } else { "GET" }
                    }) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            if cancel {
                cancellation.cancel();
            }
            let data = task.await.unwrap().structured_content.unwrap();
            assert_eq!(data["operation"]["target"]["parent_number"], 2);
            assert_eq!(
                data["operation"]["status"],
                if delay_post { "unknown" } else { "partial" }
            );
            assert_eq!(
                data["operation"]["target"]["number"],
                if delay_post { Value::Null } else { json!(7) }
            );
            assert_eq!(
                data["cause"]["kind"],
                if cancel {
                    "cancelled_error"
                } else {
                    "timeout_error"
                }
            );
            assert_eq!(f.posts().await, 1);
            peer.cancel().await.unwrap();
        }
    }
}
