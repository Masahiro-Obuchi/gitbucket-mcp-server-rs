use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::{
        operation::OutcomeStatus,
        review_post::{CreateReviewComment, HeadGuard},
    },
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
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

fn change() -> CreateReviewComment {
    CreateReviewComment {
        body: "review\n\n    code\nhard break  \nend".into(),
        path: "review & 日本.txt".into(),
        commit: "b".repeat(40),
        old_line: None,
        new_line: Some(2),
        head_guard: HeadGuard::Preflight,
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn card(c: &CreateReviewComment) -> String {
    format!("<div class='commit-comment-box' id='discussion_r42' filename=\"{}\" {}='2'><div class='commit-comment-42'><a class='username' href='/alice'>alice</a><div class='commit-commentContent-42'><p>rendered review</p></div></div></div>",escape(&c.path),if c.old_line.is_some(){"oldline"}else{"newline"})
}
fn page(cards: &str) -> String {
    format!("<span id='show-title'>PR</span><ul id='pullreq-tab'><li class='active'><a href='/owner/repo/pull/1'>Conversation<span class='badge'>{}</span></a></li><li><a href='/owner/repo/pull/1/commits'>Commits</a></li><li><a href='/owner/repo/pull/1/files'>Files</a></li></ul><div id='comment-list'><div class='issue-comment-box'><div id='issueContent'>body</div></div>{cards}</div>",usize::from(!cards.is_empty()))
}
fn patch(c: &CreateReviewComment) -> String {
    format!("diff --git a/{0} b/{0}\nindex 123..456 100644\n--- a/{0}\n+++ b/{0}\n@@ -1,3 +1,3 @@\n alpha\n-old\n+new\n omega\n",c.path)
}

struct Fixture {
    http: MockServer,
    api: GitBucketClient,
    posts: Arc<AtomicUsize>,
}
async fn fixture(c: &CreateReviewComment, mode: &str) -> Fixture {
    let http = MockServer::start().await;
    let applied = Arc::new(AtomicBool::new(mode == "existing"));
    let posts = Arc::new(AtomicUsize::new(0));
    let count = Arc::new(AtomicUsize::new(0));
    let (changed, post_change) = (mode == "preflight_race", mode == "post_race");
    let (calls, posted) = (count.clone(), applied.clone());
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/pulls/1")).respond_with(move |_:&wiremock::Request| {
        let call=calls.fetch_add(1,Ordering::SeqCst);
        ResponseTemplate::new(200).set_body_json(json!({"number":1,"title":"PR","state":"open","updated_at":if posted.load(Ordering::SeqCst){"after own comment"}else{"before"},"head":{"ref":"feature","sha":if (changed && call>=1)||(post_change&&posted.load(Ordering::SeqCst)){"c".repeat(40)}else{"b".repeat(40)}},"base":{"ref":"main","sha":"a".repeat(40)}}))
    }).mount(&http).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"alice"})))
        .mount(&http)
        .await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=fake; Path=/"),
        )
        .mount(&http)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html>Authenticated landing</html>"),
        )
        .mount(&http)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/owner/repo/patch/{}...{}",
            "a".repeat(40),
            "b".repeat(40)
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/plain")
                .set_body_string(patch(c)),
        )
        .mount(&http)
        .await;
    let (visible, rendered) = (applied.clone(), card(c));
    let missing = mode == "missing_pr";
    Mock::given(method("GET"))
        .and(path("/owner/repo/pull/1"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_string(page(
                if visible.load(Ordering::SeqCst) && !missing {
                    &rendered
                } else {
                    ""
                },
            ))
        })
        .mount(&http)
        .await;
    let (written, attempts, rendered, mode_owned) =
        (applied.clone(), posts.clone(), card(c), mode.to_owned());
    Mock::given(method("POST"))
        .and(path(format!(
            "/owner/repo/commit/{}/comment/_data/new",
            "b".repeat(40)
        )))
        .respond_with(move |_: &wiremock::Request| {
            attempts.fetch_add(1, Ordering::SeqCst);
            if mode_owned == "denied" {
                return ResponseTemplate::new(403).set_body_string("fake-secret");
            }
            written.store(true, Ordering::SeqCst);
            match mode_owned.as_str() {
                "redirect" => {
                    ResponseTemplate::new(307).insert_header("Location", "/must-not-follow")
                }
                "bad_response" => ResponseTemplate::new(200)
                    .set_body_string("<div class='commit-comment-box' id='discussion_r42'></div>"),
                _ => ResponseTemplate::new(200).set_body_string(rendered.clone()),
            }
        })
        .mount(&http)
        .await;
    let raw = if mode == "wrong_body" {
        "different".to_owned()
    } else {
        c.body.clone()
    };
    let mut response=ResponseTemplate::new(200).set_body_string(format!("<span class='error-edit-content-42'></span><textarea name='content'>{}</textarea><input type='button' class='update-comment-42'>",escape(&raw)));
    if mode == "slow_raw" {
        response = response.set_delay(Duration::from_secs(2));
    }
    if mode == "raw_denied" {
        response = ResponseTemplate::new(403).set_body_string("fake-secret");
    }
    Mock::given(method("GET"))
        .and(path("/owner/repo/commit_comments/_data/42"))
        .respond_with(response)
        .mount(&http)
        .await;
    let commit_page=format!("<span class='commit-id monospace strong'>{}</span><input type='checkbox' id='show-notes'><div id='comment-list'><div style='display:none'>{}</div></div>",if mode=="wrong_commit"{"c".repeat(40)}else{"b".repeat(40)},card(c));
    Mock::given(method("GET"))
        .and(path(format!("/owner/repo/commit/{}", "b".repeat(40))))
        .respond_with(ResponseTemplate::new(200).set_body_string(commit_page))
        .mount(&http)
        .await;
    let api = GitBucketClient::new_with_web_auth(
        &http.uri(),
        "fake-pat",
        false,
        Some("alice"),
        Some("fake-password"),
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard);
    Fixture { http, api, posts }
}

#[tokio::test]
async fn new_and_old_side_posts_preserve_raw_body_and_verify_commit_and_pr() {
    for old in [false, true] {
        let mut c = change();
        if old {
            c.new_line = None;
            c.old_line = Some(2);
        }
        let f = fixture(&c, "").await;
        let result = f
            .api
            .add_pull_request_review_comment("owner", "repo", 1, &c)
            .await
            .unwrap();
        assert_eq!(result.comment.id, "commit:42");
        assert_eq!(result.comment.body_raw.as_deref(), Some(c.body.as_str()));
        assert!(result.comment.position_verified);
        assert_eq!(result.comment.current_commit, Some(c.commit.clone()));
        assert!(
            result.verification.raw_body_verified
                && result.verification.commit_page_verified
                && result.verification.pr_page_membership_verified
        );
        assert!(
            !result.verification.atomic_head_guard
                && !result.verification.stored_pr_association_verified
                && !result.coverage.complete
        );
        assert_eq!(f.posts.load(Ordering::SeqCst), 1);
        let requests = f.http.received_requests().await.unwrap();
        for request in &requests {
            if !request.url.path().starts_with("/api/v3/") {
                assert!(!request.headers.contains_key("authorization"));
            }
            if request.method == "POST" {
                assert!(
                    request.url.path() == "/signin"
                        || request.url.path().ends_with("/comment/_data/new")
                );
            }
            assert!(!request.url.path().contains("/issues/1/comments"));
        }
        let post = requests
            .iter()
            .find(|r| r.url.path().ends_with("/comment/_data/new"))
            .unwrap();
        let form: HashMap<_, _> = url::form_urlencoded::parse(&post.body)
            .into_owned()
            .collect();
        assert_eq!(form["content"], c.body);
        assert_eq!(form["fileName"], c.path);
        assert_eq!(form["issueId"], "1");
        assert_eq!(
            form.get(if old {
                "oldLineNumber"
            } else {
                "newLineNumber"
            })
            .unwrap(),
            "2"
        );
        assert!(!form.contains_key(if old {
            "newLineNumber"
        } else {
            "oldLineNumber"
        }));
    }
}

#[tokio::test]
async fn unsupported_guards_whitespace_controls_and_invalid_inputs_do_not_send_http() {
    let http = MockServer::start().await;
    let api =
        GitBucketClient::new_with_web_auth(&http.uri(), "fake", false, Some("alice"), Some("pass"))
            .unwrap()
            .with_api_profile(ApiProfile::Standard);
    let mut cases = Vec::new();
    let mut c = change();
    c.head_guard = HeadGuard::Atomic;
    cases.push(c);
    for field in ["body", "path"] {
        for value in [" leading", "trailing ", "\rbad", "bad\0value"] {
            let mut c = change();
            if field == "body" {
                c.body = value.into();
            } else {
                c.path = value.into();
            }
            cases.push(c);
        }
    }
    let mut c = change();
    c.old_line = Some(2);
    cases.push(c);
    let mut c = change();
    c.new_line = None;
    cases.push(c);
    let mut c = change();
    c.new_line = Some(0);
    cases.push(c);
    let mut c = change();
    c.commit = "not-a-sha".into();
    cases.push(c);
    let mut c = change();
    c.path = "../outside".into();
    cases.push(c);
    for c in cases {
        assert!(api
            .add_pull_request_review_comment("owner", "repo", 1, &c)
            .await
            .is_err());
    }
    assert!(http.received_requests().await.unwrap().is_empty());
    let rest = GitBucketClient::new(&http.uri(), "fake").unwrap();
    assert!(matches!(
        rest.add_pull_request_review_comment("owner", "repo", 1, &change())
            .await,
        Err(GbMcpError::Unsupported(_))
    ));
}

#[tokio::test]
async fn wrong_commit_path_line_and_preflight_race_never_post() {
    for mode in ["head", "path", "line", "preflight_race"] {
        let c = change();
        let f = fixture(&c, mode).await;
        let mut sent = c.clone();
        match mode {
            "head" => sent.commit = "a".repeat(40),
            "path" => sent.path = "missing.txt".into(),
            "line" => sent.new_line = Some(1),
            _ => {}
        }
        assert!(f
            .api
            .add_pull_request_review_comment("owner", "repo", 1, &sent)
            .await
            .is_err());
        assert_eq!(f.posts.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn definite_denial_is_not_applied_but_redirect_and_bad_response_are_unknown() {
    for (mode, status) in [
        ("denied", OutcomeStatus::NotApplied),
        ("redirect", OutcomeStatus::Unknown),
        ("bad_response", OutcomeStatus::Unknown),
        ("existing", OutcomeStatus::Unknown),
    ] {
        let c = change();
        let f = fixture(&c, mode).await;
        let err = f
            .api
            .add_pull_request_review_comment("owner", "repo", 1, &c)
            .await
            .unwrap_err();
        assert!(!err.to_string().contains("fake-secret"));
        let GbMcpError::OperationFailed { outcome, .. } = err else {
            panic!("expected operation outcome")
        };
        assert_eq!(outcome.status, status);
        let target = outcome.target.unwrap();
        assert_eq!(target.parent_number, Some(1));
        assert_eq!(target.commit, Some(c.commit));
        assert_eq!(target.path, Some(c.path));
        if mode == "bad_response" {
            assert_eq!(target.number, Some(42));
        }
        if mode == "existing" {
            assert_eq!(target.candidate_numbers, vec![42]);
        }
        assert_eq!(f.posts.load(Ordering::SeqCst), 1);
        assert!(!f
            .http
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/must-not-follow"));
    }
}

#[tokio::test]
async fn failed_post_verification_preserves_the_id_and_never_reposts() {
    for mode in [
        "raw_denied",
        "wrong_body",
        "wrong_commit",
        "missing_pr",
        "post_race",
    ] {
        let c = change();
        let f = fixture(&c, mode).await;
        let err = f
            .api
            .add_pull_request_review_comment("owner", "repo", 1, &c)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = err else {
            panic!("expected operation outcome")
        };
        assert_ne!(outcome.status, OutcomeStatus::NotApplied);
        assert_ne!(outcome.status, OutcomeStatus::Verified);
        assert_eq!(outcome.target.unwrap().number, Some(42));
        assert_eq!(f.posts.load(Ordering::SeqCst), 1);
    }
}

async fn connect(
    server: GitBucketMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (a, b) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let server = server.serve(a).await.unwrap();
        let _ = server.waiting().await;
    });
    ().serve(b).await.unwrap()
}
fn args(c: &CreateReviewComment) -> Value {
    let mut v = serde_json::to_value(c).unwrap();
    v["owner"] = json!("owner");
    v["repo"] = json!("repo");
    v["pull_number"] = json!(1);
    v
}

#[tokio::test]
async fn mcp_post_and_readonly_direct_calls_respect_the_write_boundary() {
    let c = change();
    let f = fixture(&c, "").await;
    let server = GitBucketMcpServer::new(f.api.clone()).with_access_mode(AccessMode::ReadOnly);
    let direct = server
        .add_pull_request_review_comment(rmcp::handler::server::wrapper::Parameters(
            serde_json::from_value(args(&c)).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(
        direct.structured_content.unwrap()["kind"],
        "read_only_error"
    );
    let readonly = connect(server).await;
    let result = readonly
        .call_tool(
            CallToolRequestParams::new("add_pull_request_review_comment")
                .with_arguments(args(&c).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "read_only_error"
    );
    assert!(f.http.received_requests().await.unwrap().is_empty());
    readonly.cancel().await.unwrap();
    let client = connect(GitBucketMcpServer::new(f.api)).await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 32);
    let tool = tools
        .iter()
        .find(|t| t.name == "add_pull_request_review_comment")
        .unwrap();
    assert_eq!(
        tool.annotations.as_ref().unwrap().idempotent_hint,
        Some(false)
    );
    assert!(tool.input_schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("head_guard")));
    let result = client
        .call_tool(
            CallToolRequestParams::new("add_pull_request_review_comment")
                .with_arguments(args(&c).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.structured_content.unwrap()["comment"]["body_raw"],
        c.body
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn deadline_after_creation_keeps_id_and_unknown_without_retry() {
    let c = change();
    let f = fixture(&c, "slow_raw").await;
    let server =
        GitBucketMcpServer::with_api_and_timeout(Arc::new(f.api), Duration::from_millis(500))
            .unwrap();
    let client = connect(server).await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("add_pull_request_review_comment")
                .with_arguments(args(&c).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let data = result.structured_content.unwrap();
    assert_eq!(data["kind"], "operation_unknown");
    assert_eq!(data["operation"]["target"]["number"], 42);
    assert_eq!(data["cause"]["kind"], "timeout_error");
    assert_eq!(f.posts.load(Ordering::SeqCst), 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn saved_comment_response_disconnect_is_unknown_and_is_not_replayed() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for cut_post in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let c = change();
        let request = c.clone();
        let posts = Arc::new(AtomicUsize::new(0));
        let writes = posts.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let (c, writes) = (request.clone(), writes.clone());
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buffer = [0; 2048];
                    let header_end = loop {
                        let n = stream.read(&mut buffer).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buffer[..n]);
                        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                            break i + 4;
                        }
                        if data.len() > 65536 {
                            return;
                        }
                    };
                    let header = String::from_utf8_lossy(&data[..header_end]).into_owned();
                    let length = header
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    while data.len() < header_end + length {
                        let n = stream.read(&mut buffer).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&buffer[..n]);
                    }
                    let uri = header
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    let uri = uri.split('?').next().unwrap();
                    let is_post =
                        header.starts_with("POST ") && uri.ends_with("/comment/_data/new");
                    if is_post {
                        writes.fetch_add(1, Ordering::SeqCst);
                    }
                    if (cut_post && is_post)
                        || (!cut_post && uri.contains("/commit_comments/_data/"))
                    {
                        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 999\r\nConnection: close\r\n\r\n<div>partial saved response").await.unwrap();
                        let _ = stream.shutdown().await;
                        return;
                    }
                    let (status, extra, kind, body) = if uri == "/signin" {
                        (
                            "303 See Other",
                            "Location: /\r\nSet-Cookie: JSESSIONID=fake; Path=/\r\n",
                            "text/html",
                            String::new(),
                        )
                    } else if uri == "/api/v3/user" {
                        (
                            "200 OK",
                            "",
                            "application/json",
                            json!({"login":"alice"}).to_string(),
                        )
                    } else if uri.contains("/api/v3/repos/") {
                        ("200 OK","","application/json",json!({"number":1,"title":"PR","state":"open","head":{"ref":"feature","sha":"b".repeat(40)},"base":{"ref":"main","sha":"a".repeat(40)}}).to_string())
                    } else if uri.contains("/patch/") {
                        ("200 OK", "", "text/plain", patch(&c))
                    } else if uri == "/owner/repo/pull/1" {
                        ("200 OK", "", "text/html", page(""))
                    } else if is_post {
                        ("200 OK", "", "text/html", card(&c))
                    } else {
                        (
                            "200 OK",
                            "",
                            "text/html",
                            "<html>Authenticated landing</html>".into(),
                        )
                    };
                    let response=format!("HTTP/1.1 {status}\r\n{extra}Content-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        let api = GitBucketClient::new_with_web_auth(
            &format!("http://{address}"),
            "fake-pat",
            false,
            Some("alice"),
            Some("fake-password"),
        )
        .unwrap()
        .with_api_profile(ApiProfile::Standard);
        let err = api
            .add_pull_request_review_comment("owner", "repo", 1, &c)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = err else {
            panic!("expected unknown write")
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        assert_eq!(
            outcome.target.unwrap().number,
            if cut_post { None } else { Some(42) }
        );
        assert_eq!(posts.load(Ordering::SeqCst), 1);
        server.abort();
    }
}
