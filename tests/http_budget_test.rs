#[allow(dead_code)]
mod common;
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, http::HttpSettings, web::GitBucketWebSession},
    error::GbMcpError,
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn settings() -> HttpSettings {
    HttpSettings {
        connect_timeout: Duration::from_millis(100),
        request_timeout: Duration::from_millis(200),
        merge_timeout: Duration::from_secs(2),
        tool_timeout: Duration::from_secs(3),
        json_limit_bytes: 1024,
        html_limit_bytes: 1024,
    }
}
fn client(base: &str, settings: HttpSettings) -> GitBucketClient {
    GitBucketClient::new_with_web_auth_and_http_settings(
        base,
        "fake-token",
        true,
        Some("user"),
        Some("fake-password"),
        settings,
    )
    .unwrap()
}
fn cause(error: &GbMcpError) -> &GbMcpError {
    match error {
        GbMcpError::OperationOutcomeUnknown { cause, .. } => cause.as_ref(),
        error => error,
    }
}

/// One accepted request, fully read before the response starts. No external network.
async fn raw_server(parts: Vec<(Duration, Vec<u8>)>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            if n == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..n]);
            if let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + length {
                    break;
                }
            }
        }
        for (delay, bytes) in parts {
            tokio::time::sleep(delay).await;
            if stream.write_all(&bytes).await.is_err() {
                return;
            }
        }
    });
    (url, task)
}

#[tokio::test]
async fn delayed_response_and_tls_connect_have_bounded_waits() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(1)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let started = tokio::time::Instant::now();
    assert!(matches!(
        client(&server.uri(), settings())
            .get::<Value>("/slow")
            .await,
        Err(GbMcpError::Timeout { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    // A TCP peer that accepts but never completes TLS exercises connect_timeout.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let mut policy = settings();
    policy.request_timeout = Duration::from_secs(5);
    let started = tokio::time::Instant::now();
    assert!(matches!(
        client(&url, policy).get::<Value>("/user").await,
        Err(GbMcpError::Timeout { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    task.abort();
}

#[tokio::test]
async fn body_deadline_includes_time_after_headers_and_between_chunks() {
    let (url, task) = raw_server(vec![
        (
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\n{\r\n".to_vec(),
        ),
        (Duration::from_secs(1), b"1\r\n}\r\n0\r\n\r\n".to_vec()),
    ])
    .await;
    assert!(matches!(
        client(&url, settings()).get::<Value>("/body").await,
        Err(GbMcpError::Timeout { .. })
    ));
    task.abort();
}

#[tokio::test]
async fn response_limits_cover_declared_chunked_and_unframed_bodies() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 9999999\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n1234\r\n4\r\n5678\r\n4\r\n9012\r\n0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n123456789012".to_vec(),
    ] {
        let (url,task)=raw_server(vec![(Duration::ZERO,response)]).await;
        let mut policy=settings();policy.json_limit_bytes=8;
        assert!(matches!(client(&url,policy).get::<Value>("/large").await,Err(GbMcpError::ResponseTooLarge{kind:"JSON",limit:8})));
        task.await.unwrap();
    }
    let (url, task) = raw_server(vec![(
        Duration::ZERO,
        b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n12345678".to_vec(),
    )])
    .await;
    let mut policy = settings();
    policy.json_limit_bytes = 8;
    assert_eq!(
        client(&url, policy).get::<u64>("/exact").await.unwrap(),
        12345678
    );
    task.await.unwrap();
}

#[tokio::test]
async fn mutations_with_timeout_disconnect_invalid_json_or_oversize_are_unknown_without_retry() {
    let server = MockServer::start().await;
    for (name, response) in [
        (
            "timeout",
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(1)),
        ),
        (
            "json",
            ResponseTemplate::new(200).set_body_string("invalid-json"),
        ),
        (
            "large",
            ResponseTemplate::new(200).set_body_string("x".repeat(1025)),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v3/{name}")))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let error = client(&server.uri(), settings())
            .post::<Value, _>(&format!("/{name}"), &json!({"saved":true}))
            .await
            .unwrap_err();
        assert!(matches!(error, GbMcpError::OperationOutcomeUnknown { .. }));
        match name {
            "timeout" => assert!(matches!(cause(&error), GbMcpError::Timeout { .. })),
            "json" => assert!(matches!(cause(&error), GbMcpError::Json(_))),
            _ => assert!(matches!(cause(&error), GbMcpError::ResponseTooLarge { .. })),
        }
    }
    // The helper reads the full write, then disconnects before returning valid JSON.
    let (url, task) = raw_server(vec![(
        Duration::ZERO,
        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{".to_vec(),
    )])
    .await;
    let error = client(&url, settings())
        .post::<Value, _>("/saved", &json!({"saved":true}))
        .await
        .unwrap_err();
    assert!(matches!(error, GbMcpError::OperationOutcomeUnknown { .. }));
    task.await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn rest_mutations_do_not_follow_redirects_or_replay_writes() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/redirect-target"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server.uri(), settings());
    for verb in ["POST", "PATCH", "PUT", "DELETE"] {
        for status in [301, 302, 303, 307, 308] {
            let route = format!("/write-{verb}-{status}");
            Mock::given(method(verb))
                .and(path(format!("/api/v3{route}")))
                .and(wiremock::matchers::header(
                    "Authorization",
                    "token fake-token",
                ))
                .and(wiremock::matchers::header("Accept", "application/json"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .insert_header("Location", "/api/v3/redirect-target"),
                )
                .expect(1)
                .mount(&server)
                .await;
            let body = json!({"saved": true});
            let result = match verb {
                "POST" => client.post::<Value, _>(&route, &body).await,
                "PATCH" => client.patch::<Value, _>(&route, &body).await,
                "PUT" => client.put::<Value, _>(&route, &body).await,
                "DELETE" => client.delete(&route).await.map(|_| Value::Null),
                _ => unreachable!(),
            };
            let error = result.unwrap_err();
            assert!(
                matches!(error, GbMcpError::OperationOutcomeUnknown { .. }),
                "{verb} {status}: {error:?}"
            );
            assert!(
                matches!(cause(&error), GbMcpError::Api { status: actual, .. } if *actual == status),
                "{verb} {status}: {error:?}"
            );
        }
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 20);
}

#[tokio::test]
async fn rest_reads_still_follow_safe_redirects() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/redirect"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/api/v3/target"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/target"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server.uri(), settings())
            .get::<Value>("/redirect")
            .await
            .unwrap(),
        json!({"ok": true})
    );
}

#[tokio::test]
async fn redirected_rest_write_does_not_enter_issue_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(ResponseTemplate::new(303).insert_header("Location", "/api/v3/missing"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;
    let peer = protocol(client(&server.uri(), settings()), Duration::from_secs(3)).await;
    let result = peer
        .call_tool(
            CallToolRequestParams::new("update_issue").with_arguments(
                json!({"owner": "owner", "repo": "repo", "issue_number": 1, "title": "new"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "outcome_unknown"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    peer.cancel().await.unwrap();
}

#[tokio::test]
async fn errors_keep_safe_status_and_wrapped_404_still_allows_api_profile() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/wrapped"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"status":404,"body":"secret"})),
        )
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server.uri(), settings())
            .get::<Value>("/wrapped")
            .await,
        Err(GbMcpError::Api { status: 404, .. })
    ));
    Mock::given(method("GET"))
        .and(path("/api/v3/error"))
        .respond_with(ResponseTemplate::new(403).set_body_string("secret".repeat(10000)))
        .mount(&server)
        .await;
    let error = client(&server.uri(), settings())
        .get::<Value>("/error")
        .await
        .unwrap_err();
    assert!(matches!(error, GbMcpError::Api { status: 403, .. }));
    assert!(!error.to_string().contains("secret"));
}

#[tokio::test]
async fn merge_has_a_longer_explicit_request_budget() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/owner/repo/pulls/1/merge"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"merged":true,"message":"ok"}))
                .set_delay(Duration::from_millis(400)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = client(&server.uri(), settings())
        .merge_pull_request(
            "owner",
            "repo",
            1,
            &gitbucket_mcp_server::models::pull_request::MergePullRequest {
                commit_message: None,
                merge_method: None,
                sha: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.merged, Some(true));
}

async fn web(server: &MockServer, policy: HttpSettings) -> GitBucketWebSession {
    common::web_identity(server, "", "user").await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=test; Path=/"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("signed in"))
        .mount(server)
        .await;
    client(&server.uri(), policy).web_session().await.unwrap()
}

#[tokio::test]
async fn web_uses_the_same_deadlines_and_bounded_html_for_login_reads_and_writes() {
    let server = MockServer::start().await;
    let session = web(&server, settings()).await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1025)))
        .mount(&server)
        .await;
    assert!(matches!(
        session.find_label_id("owner", "repo", "name").await,
        Err(GbMcpError::ResponseTooLarge { kind: "HTML", .. })
    ));
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1025)))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        session.edit_issue_title("owner", "repo", 1, "title").await,
        Err(GbMcpError::OperationOutcomeUnknown { .. })
    ));
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/milestones/1/open"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1)))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        session
            .update_milestone_state("owner", "repo", 1, "open")
            .await,
        Err(GbMcpError::OperationOutcomeUnknown { .. })
    ));
    let server = MockServer::start().await;
    common::web_identity(&server, "", "user").await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=test; Path=/"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1025)))
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server.uri(), settings()).web_session().await,
        Err(GbMcpError::ResponseTooLarge { .. })
    ));
}

async fn protocol(
    client: GitBucketClient,
    timeout: Duration,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server_transport, client_transport) = tokio::io::duplex(16384);
    tokio::spawn(async move {
        let service = GitBucketMcpServer::with_api_and_timeout(Arc::new(client), timeout)
            .unwrap()
            .serve(server_transport)
            .await
            .unwrap();
        let _ = service.waiting().await;
    });
    ().serve(client_transport).await.unwrap()
}

#[tokio::test]
async fn whole_tool_budget_limits_pagination_and_does_not_return_partial_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(|request: &wiremock::Request| {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
                .parse::<u64>()
                .unwrap();
            ResponseTemplate::new(200)
                .set_body_json(json!([{"number":page,"title":"issue","state":"open"}]))
                .set_delay(Duration::from_millis(80))
        })
        .mount(&server)
        .await;
    let protocol = protocol(
        client(&server.uri(), settings()),
        Duration::from_millis(210),
    )
    .await;
    let result = protocol
        .call_tool(
            CallToolRequestParams::new("list_issues").with_arguments(
                json!({"owner":"owner","repo":"repo"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["kind"], "timeout_error");
    assert!(server.received_requests().await.unwrap().len() <= 3);
    protocol.cancel().await.unwrap();
}

#[tokio::test]
async fn whole_tool_write_timeout_is_unknown_and_request_tracking_is_isolated() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(1)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(1)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let mut policy = settings();
    policy.request_timeout = Duration::from_secs(2);
    let protocol = protocol(client(&server.uri(), policy), Duration::from_millis(150)).await;
    let write = CallToolRequestParams::new("create_issue").with_arguments(
        json!({"owner":"owner","repo":"repo","title":"test"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let read = CallToolRequestParams::new("get_authenticated_user");
    let (write, read) = tokio::join!(protocol.call_tool(write), protocol.call_tool(read));
    assert_eq!(
        write.unwrap().structured_content.unwrap()["kind"],
        "outcome_unknown"
    );
    assert_eq!(
        read.unwrap().structured_content.unwrap()["kind"],
        "timeout_error"
    );
    protocol.cancel().await.unwrap();
}

#[tokio::test]
async fn cancellation_stops_in_flight_reads_and_writes_without_replay() {
    use rmcp::{
        model::{
            CallToolRequest, CancelledNotification, CancelledNotificationParam, ClientNotification,
            ClientRequest,
        },
        service::PeerRequestOptions,
    };
    for writing in [false, true] {
        let server = MockServer::start().await;
        let arrived = Arc::new(tokio::sync::Notify::new());
        let notify = arrived.clone();
        Mock::given(method(if writing { "POST" } else { "GET" }))
            .respond_with(move |_: &wiremock::Request| {
                notify.notify_one();
                ResponseTemplate::new(200)
                    .set_body_json(json!({}))
                    .set_delay(Duration::from_secs(2))
            })
            .expect(1)
            .mount(&server)
            .await;
        let mut policy = settings();
        policy.request_timeout = Duration::from_secs(3);
        let peer = protocol(client(&server.uri(), policy), Duration::from_secs(5)).await;
        let params = if writing {
            CallToolRequestParams::new("create_issue").with_arguments(
                json!({"owner":"owner","repo":"repo","title":"test"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        } else {
            CallToolRequestParams::new("get_authenticated_user")
        };
        let handle = peer
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(params)),
                PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), arrived.notified())
            .await
            .unwrap();
        peer.send_notification(ClientNotification::CancelledNotification(
            CancelledNotification::new(CancelledNotificationParam {
                request_id: handle.id.clone(),
                reason: Some("test cancellation".into()),
            }),
        ))
        .await
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), handle.await_response())
            .await
            .unwrap();
        assert!(matches!(
            result,
            Err(rmcp::service::ServiceError::Cancelled { .. })
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        peer.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn standard_issue_prewrite_read_timeout_is_not_unknown() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1)))
        .expect(1)
        .mount(&server)
        .await;
    let mut policy = settings();
    policy.request_timeout = Duration::from_secs(2);
    let peer = protocol(
        client(&server.uri(), policy)
            .with_api_profile(gitbucket_mcp_server::api::profile::ApiProfile::Standard),
        Duration::from_millis(150),
    )
    .await;
    let result = peer
        .call_tool(
            CallToolRequestParams::new("update_issue").with_arguments(
                json!({"owner":"owner","repo":"repo","issue_number":1,"title":"changed"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result.structured_content.unwrap()["kind"], "timeout_error");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    peer.cancel().await.unwrap();
}

#[tokio::test]
async fn successful_web_write_then_failed_verification_is_unknown() {
    let server = MockServer::start().await;
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = reads.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(move |_: &wiremock::Request| {
            if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"number":1,"title":"old","state":"open"}))
            } else {
                ResponseTemplate::new(200)
                    .set_body_json(json!({}))
                    .set_delay(Duration::from_secs(1))
            }
        })
        .mount(&server)
        .await;
    let _ = web(&server, settings()).await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/_data/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"title":"new","content":""})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(
            ResponseTemplate::new(303).insert_header("Location", "/owner/repo/issues/_data/1"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server.uri(), settings())
        .with_api_profile(gitbucket_mcp_server::api::profile::ApiProfile::Standard)
        .update_issue(
            "owner",
            "repo",
            1,
            &gitbucket_mcp_server::models::issue::UpdateIssue {
                title: Some("new".into()),
                body: None,
                state: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, GbMcpError::OperationOutcomeUnknown { .. }));
    assert!(matches!(cause(&error), GbMcpError::Timeout { .. }));
}

#[tokio::test]
async fn server_classifies_cancelled_write_as_unknown_before_client_discards_response() {
    use rmcp::{service::RequestContext, ServerHandler};
    let http = MockServer::start().await;
    let arrived = Arc::new(tokio::sync::Notify::new());
    let notify = arrived.clone();
    Mock::given(method("POST"))
        .respond_with(move |_: &wiremock::Request| {
            notify.notify_one();
            ResponseTemplate::new(200).set_delay(Duration::from_secs(2))
        })
        .expect(1)
        .mount(&http)
        .await;
    let mut policy = settings();
    policy.request_timeout = Duration::from_secs(3);
    let server = GitBucketMcpServer::new(client(&http.uri(), policy));
    let server_clone = server.clone();
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let running = server_clone.serve(server_transport).await.unwrap();
        let _ = tx.send(running.peer().clone());
        let _ = running.waiting().await;
    });
    let client = ().serve(client_transport).await.unwrap();
    let context = RequestContext::new(rmcp::model::RequestId::Number(123), rx.await.unwrap());
    let ct = context.ct.clone();
    let params = CallToolRequestParams::new("create_issue").with_arguments(
        json!({"owner":"owner","repo":"repo","title":"test"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let task = tokio::spawn(async move { server.call_tool(params, context).await.unwrap() });
    tokio::time::timeout(Duration::from_secs(2), arrived.notified())
        .await
        .unwrap();
    ct.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["kind"],
        "outcome_unknown"
    );
    assert_eq!(http.received_requests().await.unwrap().len(), 1);
    client.cancel().await.unwrap();
}
