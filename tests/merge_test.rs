use gitbucket_mcp_server::{
    api::client::GitBucketClient,
    error::GbMcpError,
    models::{
        operation::OutcomeStatus,
        pull_request::{MergeHeadGuard, MergeMethod, MergePullRequest},
    },
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn request(method: MergeMethod) -> MergePullRequest {
    MergePullRequest {
        merge_method: method,
        expected_head_sha: Some("b".repeat(40)),
        head_guard: MergeHeadGuard::Preflight,
        commit_message: None,
    }
}
fn pr(mode: &str, after: bool) -> Value {
    let changed = (mode == "head_before" && !after) || (mode == "head_after" && after);
    json!({"number":1,"title":"PR","state":if after&&mode!="ignored"||mode=="closed"{"closed"}else{"open"},"merged":after&&mode!="ignored","draft":if mode=="unknown_draft"{Value::Null}else{json!(mode=="draft")},"mergeable":if mode=="unmergeable"{json!(false)}else{Value::Null},"head":{"ref":"feature","sha":if changed{"d".repeat(40)}else{"b".repeat(40)}},"base":{"ref":if mode=="base_after"&&after{"other"}else{"main"},"sha":"a".repeat(40)}})
}
struct Fixture {
    http: MockServer,
    api: GitBucketClient,
    puts: Arc<AtomicUsize>,
}
async fn fixture(change: &MergePullRequest, mode: &str, native: bool) -> Fixture {
    let http = MockServer::start().await;
    let applied = Arc::new(AtomicBool::new(false));
    let puts = Arc::new(AtomicUsize::new(0));
    let (a, m, c) = (
        applied.clone(),
        mode.to_owned(),
        Arc::new(AtomicUsize::new(0)),
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1"))
        .respond_with(move |_: &wiremock::Request| {
            let after = a.load(Ordering::SeqCst);
            if after && m == "read_denied" {
                return ResponseTemplate::new(403).set_body_string("fake-secret");
            }
            let mode = if m == "preflight_race" && c.fetch_add(1, Ordering::SeqCst) > 0 {
                "head_before"
            } else {
                &m
            };
            let r = ResponseTemplate::new(200).set_body_json(pr(mode, after));
            if after && m == "slow_read" {
                r.set_delay(Duration::from_secs(2))
            } else {
                r
            }
        })
        .mount(&http)
        .await;
    let (a, m) = (applied.clone(), mode.to_owned());
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/branches/main"))
        .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"name":"main","commit":{"sha":if a.load(Ordering::SeqCst) {if m=="base_moved"{"d".repeat(40)}else{"c".repeat(40)}}else{"a".repeat(40)}}}))).mount(&http).await;
    let (a, p, m) = (applied.clone(), puts.clone(), mode.to_owned());
    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/owner/repo/pulls/1/merge"))
        .respond_with(move |_: &wiremock::Request| {
            p.fetch_add(1, Ordering::SeqCst);
            if m == "denied" {
                return ResponseTemplate::new(405).set_body_json(json!({"message":"fake-secret"}));
            }
            a.store(true, Ordering::SeqCst);
            if m == "redirect" {
                return ResponseTemplate::new(307).insert_header("Location", "/must-not-follow");
            }
            let sha = if m == "malformed" {
                "not-a-sha".into()
            } else if native {
                format!(
                    "MergeResult(AnyObjectId[{}],List({}))",
                    "c".repeat(40),
                    "b".repeat(40)
                )
            } else {
                "c".repeat(40)
            };
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":sha,"merged":m!="negative","message":"merged"}))
        })
        .mount(&http)
        .await;
    let parents = if mode == "wrong_parents" || mode == "sha_ignored" {
        json!([{"sha":"a".repeat(40)},{"sha":"d".repeat(40)}])
    } else if change.merge_method == MergeMethod::Merge {
        json!([{"sha":"a".repeat(40)},{"sha":"b".repeat(40)}])
    } else {
        json!([{"sha":"a".repeat(40)}])
    };
    Mock::given(method("GET")).and(path(format!("/api/v3/repos/owner/repo/commits/{}","c".repeat(40))))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":if mode=="wrong_commit"{"d".repeat(40)}else{"c".repeat(40)},"parents":parents}))).mount(&http).await;
    let api = GitBucketClient::new(&http.uri(), "fake-pat").unwrap();
    Fixture { http, api, puts }
}

#[tokio::test]
async fn all_strategies_use_native_names_and_verify_only_observable_facts() {
    for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
        for native in [false, true] {
            let change = request(method);
            let f = fixture(&change, "normal", native).await;
            let result = f
                .api
                .merge_pull_request("owner", "repo", 1, &change)
                .await
                .unwrap();
            assert!(result.merged);
            assert_eq!(result.sha, "c".repeat(40));
            assert_eq!(result.requested_merge_method, method);
            let v = result.verification;
            assert_eq!(v.head_guard, MergeHeadGuard::Preflight);
            assert!(!v.atomic_head_guard);
            assert_eq!(v.mergeable_before, None);
            assert!(v.pr_state_verified && v.base_ref_verified && v.commit_parents_verified);
            assert_eq!(v.merged_head_verified, method == MergeMethod::Merge);
            assert!(!v.commit_message_verified);
            assert!(!v.repository_policy_preflight_verified);
            let requests = f.http.received_requests().await.unwrap();
            let writes: Vec<_> = requests.iter().filter(|r| r.method == "PUT").collect();
            assert_eq!(writes.len(), 1);
            let body: Value = serde_json::from_slice(&writes[0].body).unwrap();
            assert_eq!(body["merge_method"], method.wire_name());
            assert!(body.get("sha").is_none());
            assert!(body.get("expected_head_sha").is_none());
            assert!(requests
                .iter()
                .all(|r| r.url.path().starts_with("/api/v3/")));
        }
    }
}

#[tokio::test]
async fn atomic_invalid_guards_and_ignored_rebase_messages_never_reach_http() {
    let http = MockServer::start().await;
    let api = GitBucketClient::new(&http.uri(), "fake").unwrap();
    let mut cases = Vec::new();
    let mut c = request(MergeMethod::Merge);
    c.head_guard = MergeHeadGuard::Atomic;
    cases.push(c);
    let mut c = request(MergeMethod::Merge);
    c.head_guard = MergeHeadGuard::None;
    cases.push(c);
    let mut c = request(MergeMethod::Merge);
    c.expected_head_sha = None;
    cases.push(c);
    for sha in ["bad".to_owned(), "B".repeat(40)] {
        let mut c = request(MergeMethod::Merge);
        c.expected_head_sha = Some(sha);
        cases.push(c);
    }
    for message in ["ignored", ""] {
        let mut c = request(MergeMethod::Rebase);
        c.commit_message = Some(message.into());
        cases.push(c);
    }
    for c in cases {
        assert!(api
            .merge_pull_request("owner", "repo", 1, &c)
            .await
            .is_err());
    }
    assert!(api
        .merge_pull_request("owner", "repo", 0, &MergePullRequest::default())
        .await
        .is_err());
    assert!(http.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn draft_unknown_draft_conflicts_and_changed_preflight_do_not_merge() {
    for mode in [
        "draft",
        "unknown_draft",
        "closed",
        "unmergeable",
        "head_before",
        "preflight_race",
    ] {
        let change = request(MergeMethod::Merge);
        let f = fixture(&change, mode, false).await;
        assert!(
            f.api
                .merge_pull_request("owner", "repo", 1, &change)
                .await
                .is_err(),
            "{mode}"
        );
        assert_eq!(f.puts.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn prohibited_strategy_is_a_definite_rejection_without_another_strategy() {
    for method in [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase] {
        let change = request(method);
        let f = fixture(&change, "denied", false).await;
        let error = f
            .api
            .merge_pull_request("owner", "repo", 1, &change)
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("fake-secret"));
        let GbMcpError::OperationFailed { outcome, cause } = error else {
            panic!("missing operation");
        };
        assert_eq!(outcome.status, OutcomeStatus::NotApplied);
        assert!(matches!(*cause, GbMcpError::Api { status: 405, .. }));
        assert_eq!(f.puts.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn failed_or_changed_observations_keep_result_commit_and_never_replay() {
    for mode in [
        "read_denied",
        "head_after",
        "base_after",
        "ignored",
        "wrong_commit",
        "wrong_parents",
        "sha_ignored",
        "base_moved",
        "negative",
        "malformed",
        "redirect",
    ] {
        let change = request(MergeMethod::Merge);
        let f = fixture(&change, mode, false).await;
        let error = f
            .api
            .merge_pull_request("owner", "repo", 1, &change)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing operation {mode}");
        };
        assert!(
            !matches!(
                outcome.status,
                OutcomeStatus::NotApplied | OutcomeStatus::Verified
            ),
            "{mode}"
        );
        if mode == "ignored" {
            assert_eq!(outcome.status, OutcomeStatus::Unknown);
        }
        let target = outcome.target.unwrap();
        assert_eq!(target.number, Some(1));
        assert_eq!(target.commit, Some("b".repeat(40)));
        assert_eq!(target.name.as_deref(), Some("main"));
        assert_eq!(
            target.result_commit,
            if matches!(mode, "malformed" | "redirect") {
                None
            } else {
                Some("c".repeat(40))
            },
            "{mode}"
        );
        assert_eq!(f.puts.load(Ordering::SeqCst), 1);
        assert!(!f
            .http
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/must-not-follow"));
    }
}

async fn connect(
    server: GitBucketMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (a, b) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let s = server.serve(a).await.unwrap();
        let _ = s.waiting().await;
    });
    ().serve(b).await.unwrap()
}
#[tokio::test]
async fn mcp_schema_preserves_plain_merge_and_readonly_blocks_explicit_guards() {
    let c = MergePullRequest::default();
    let f = fixture(&c, "normal", true).await;
    let client = connect(GitBucketMcpServer::new(f.api)).await;
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|t| t.name == "merge_pull_request")
        .unwrap();
    assert_eq!(tool.input_schema["additionalProperties"], false);
    for field in ["merge_method", "expected_head_sha", "head_guard"] {
        assert!(tool.input_schema["properties"].get(field).is_some());
    }
    let atomic=client.call_tool(CallToolRequestParams::new("merge_pull_request").with_arguments(json!({"owner":"owner","repo":"repo","pull_number":1,"head_guard":"atomic","expected_head_sha":"b".repeat(40)}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(
        atomic.structured_content.unwrap()["kind"],
        "unsupported_error"
    );
    for extra in [
        json!({"sha":"b".repeat(40)}),
        json!({"merge_method":"merge-commit"}),
    ] {
        let mut args = json!({"owner":"owner","repo":"repo","pull_number":1});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(client
            .call_tool(
                CallToolRequestParams::new("merge_pull_request")
                    .with_arguments(args.as_object().unwrap().clone())
            )
            .await
            .is_err());
    }
    assert!(f.http.received_requests().await.unwrap().is_empty());
    let r = client
        .call_tool(
            CallToolRequestParams::new("merge_pull_request").with_arguments(
                json!({"owner":"owner","repo":"repo","pull_number":1})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(false));
    assert_eq!(
        r.structured_content.unwrap()["verification"]["head_guard"],
        "none"
    );
    client.cancel().await.unwrap();
    let http = MockServer::start().await;
    let api = GitBucketClient::new(&http.uri(), "fake").unwrap();
    let client = connect(GitBucketMcpServer::new(api).with_access_mode(AccessMode::ReadOnly)).await;
    let r=client.call_tool(CallToolRequestParams::new("merge_pull_request").with_arguments(json!({"owner":"owner","repo":"repo","pull_number":1,"head_guard":"atomic","expected_head_sha":"b".repeat(40)}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(r.structured_content.unwrap()["kind"], "read_only_error");
    assert!(http.received_requests().await.unwrap().is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_deadline_after_a_successful_put_retains_result_commit() {
    let c = request(MergeMethod::Merge);
    let f = fixture(&c, "slow_read", false).await;
    let client = connect(
        GitBucketMcpServer::with_api_and_timeout(Arc::new(f.api), Duration::from_millis(250))
            .unwrap(),
    )
    .await;
    let r=client.call_tool(CallToolRequestParams::new("merge_pull_request").with_arguments(json!({"owner":"owner","repo":"repo","pull_number":1,"head_guard":"preflight","expected_head_sha":"b".repeat(40)}).as_object().unwrap().clone())).await.unwrap();
    let data = r.structured_content.unwrap();
    assert_eq!(data["kind"], "operation_unknown");
    assert_eq!(data["cause"]["kind"], "timeout_error");
    assert_eq!(data["operation"]["target"]["result_commit"], "c".repeat(40));
    assert_eq!(f.puts.load(Ordering::SeqCst), 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn saved_merge_socket_disconnect_preserves_uncertainty_without_replaying_put() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for cut_put in [true, false] {
        let c = request(MergeMethod::Merge);
        let f = fixture(&c, "normal", false).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let upstream = f.http.uri();
        let proxy = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let upstream = upstream.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buffer = [0u8; 2048];
                    let header_end = loop {
                        let n = stream.read(&mut buffer).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buffer[..n]);
                        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                            break i + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&data[..header_end]).into_owned();
                    let line = headers.lines().next().unwrap();
                    let mut parts = line.split_whitespace();
                    let verb = parts.next().unwrap();
                    let uri = parts.next().unwrap();
                    let length = headers
                        .lines()
                        .filter_map(|l| l.split_once(':'))
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    while data.len() < header_end + length {
                        let n = stream.read(&mut buffer).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buffer[..n]);
                    }
                    let response = reqwest::Client::new()
                        .request(verb.parse().unwrap(), format!("{upstream}{uri}"))
                        .body(data[header_end..].to_vec())
                        .send()
                        .await
                        .unwrap();
                    let status = response.status();
                    let body = response.bytes().await.unwrap();
                    if (cut_put && verb == "PUT") || (!cut_put && uri.contains("/commits/")) {
                        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999\r\nConnection: close\r\n\r\n{\"partial\":").await.unwrap();
                    } else {
                        let h=format!("HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",status.as_u16(),status.canonical_reason().unwrap_or("OK"),body.len());
                        stream.write_all(h.as_bytes()).await.unwrap();
                        stream.write_all(&body).await.unwrap();
                    }
                    let _ = stream.shutdown().await;
                });
            }
        });
        let api = GitBucketClient::new(&format!("http://{address}"), "fake").unwrap();
        let error = api
            .merge_pull_request("owner", "repo", 1, &c)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing operation");
        };
        assert_eq!(
            outcome.status,
            if cut_put {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::Partial
            }
        );
        assert_eq!(
            outcome.target.unwrap().result_commit,
            if cut_put { None } else { Some("c".repeat(40)) }
        );
        assert_eq!(f.puts.load(Ordering::SeqCst), 1);
        proxy.abort();
    }
}
