mod common;

use common::TestServer;
use gitbucket_mcp_server::{
    api::profile::ApiProfile,
    error::GbMcpError,
    models::comment::UpdateComment,
    models::operation::{FieldStatus, OutcomeStatus},
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, ResponseTemplate,
};

const ROUTE: &str = "/api/v3/repos/owner/repo/issues/comments/7";
const RAW: &str = "  日本語 **Markdown**  \n\n    code\n";
fn comment(author: &str, body: &str) -> Value {
    json!({"id":7,"body":body,"user":{"login":author,"email":null,"type":null,"site_admin":null,"created_at":null,"avatar_url":null,"url":null,"html_url":null},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z","html_url":"https://example.test/owner/repo/issues/2#comment-7"})
}
async fn get(http: &TestServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(response)
        .expect(1)
        .mount(&http.mock_server)
        .await;
}

#[tokio::test]
async fn get_preserves_raw_body_author_dates_and_html_link() {
    let http = TestServer::start().await;
    get(
        &http,
        ResponseTemplate::new(200).set_body_json(comment("author", RAW)),
    )
    .await;
    let result = http
        .client()
        .get_issue_comment("owner", "repo", 7)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        comment("author", RAW)
    );
    let requests = http.mock_server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers["authorization"], "token test-token");
}

#[tokio::test]
async fn update_returns_get_author_instead_of_upstream_patch_editor() {
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        let http = TestServer::start().await;
        Mock::given(method("PATCH"))
            .and(path(ROUTE))
            .and(header("authorization", "token test-token"))
            .and(body_json(json!({"body":RAW})))
            .respond_with(ResponseTemplate::new(200).set_body_json(comment("editor", RAW)))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        get(
            &http,
            ResponseTemplate::new(200).set_body_json(comment("original-author", RAW)),
        )
        .await;
        let result = http
            .client()
            .with_api_profile(profile)
            .update_issue_comment("owner", "repo", 7, &UpdateComment { body: RAW.into() })
            .await
            .unwrap();
        assert_eq!(result.user.unwrap().login, "original-author");
        assert_eq!(result.body.as_deref(), Some(RAW));
        let requests = http.mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method.as_str(), "PATCH");
        assert!(requests.iter().all(|r| r.url.query().is_none()));
    }
}

#[tokio::test]
async fn update_mismatch_has_target_and_field_evidence_without_retry() {
    let http = TestServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment("editor", RAW)))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    get(
        &http,
        ResponseTemplate::new(200).set_body_json(comment("author", "concurrent replacement")),
    )
    .await;
    let error = http
        .client()
        .update_issue_comment("owner", "repo", 7, &UpdateComment { body: RAW.into() })
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing write evidence")
    };
    assert_eq!(outcome.status, OutcomeStatus::NotVerified);
    assert_eq!(outcome.target.unwrap().number, Some(7));
    assert_eq!(outcome.fields[0].field, "body");
    assert_eq!(outcome.fields[0].status, FieldStatus::Mismatched);
}

#[tokio::test]
async fn rejected_edits_do_not_verify_or_fallback() {
    for status in [401, 403, 404] {
        let http = TestServer::start().await;
        Mock::given(method("PATCH"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let error = http
            .standard_client()
            .update_issue_comment("owner", "repo", 7, &UpdateComment { body: RAW.into() })
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing write evidence")
        };
        assert_eq!(outcome.status, OutcomeStatus::NotApplied);
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn failed_post_edit_get_is_unknown_even_for_404() {
    for status in [401, 403, 404, 500] {
        let http = TestServer::start().await;
        Mock::given(method("PATCH"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(200).set_body_json(comment("editor", RAW)))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        get(&http, ResponseTemplate::new(status)).await;
        let error = http
            .client()
            .update_issue_comment("owner", "repo", 7, &UpdateComment { body: RAW.into() })
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing write evidence")
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        assert_eq!(outcome.target.unwrap().number, Some(7));
    }
}

async fn deletion_reads(http: &TestServer, id: u64, after: ResponseTemplate, repo_status: u16) {
    let reads = std::sync::atomic::AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/issues/comments/{id}"
        )))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                let mut value = comment("author", RAW);
                value["id"] = json!(id);
                ResponseTemplate::new(200).set_body_json(value)
            } else {
                after.clone()
            }
        })
        .expect(2)
        .mount(&http.mock_server)
        .await;
    visible_repository(http, repo_status).await;
}
async fn visible_repository(http: &TestServer, status: u16) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo"))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_json(json!({"name":"repo","full_name":"owner/repo","private":true})),
        )
        .mount(&http.mock_server)
        .await;
}

#[tokio::test]
async fn delete_verifies_absence_and_visibility_without_trusting_response_body() {
    for body in ["7", "", "unexpected"] {
        let http = TestServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        deletion_reads(&http, 7, ResponseTemplate::new(404), 200).await;
        http.client()
            .delete_issue_comment("owner", "repo", 7)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn id_shaped_error_status_can_follow_a_saved_delete() {
    for id in [301, 401, 403, 404, 500] {
        let http = TestServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(format!(
                "/api/v3/repos/owner/repo/issues/comments/{id}"
            )))
            .respond_with(ResponseTemplate::new(id as u16))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        deletion_reads(&http, id, ResponseTemplate::new(404), 200).await;
        http.client()
            .delete_issue_comment("owner", "repo", id)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn malformed_http_status_after_a_saved_delete_is_verified_without_replaying() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = tokio::spawn(async move {
        for index in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
                assert!(request.len() < 16384);
            }
            let line = std::str::from_utf8(&request)
                .unwrap()
                .lines()
                .next()
                .unwrap();
            let expected = match index {
                0 | 2 => format!("GET {ROUTE} HTTP/1.1"),
                1 => format!("DELETE {ROUTE} HTTP/1.1"),
                _ => "GET /api/v3/repos/owner/repo HTTP/1.1".into(),
            };
            assert_eq!(line, expected);
            let body = match index {
                0 => comment("author", RAW).to_string(),
                3 => json!({"name":"repo","full_name":"owner/repo","private":true}).to_string(),
                _ => String::new(),
            };
            let status = match index {
                1 => "007 007",
                2 => "404 Not Found",
                _ => "200 OK",
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    let client = gitbucket_mcp_server::api::client::GitBucketClient::new(
        &format!("http://{address}"),
        "test-token",
    )
    .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.delete_issue_comment("owner", "repo", 7),
    )
    .await
    .unwrap()
    .unwrap();
    fixture.await.unwrap();
}

#[tokio::test]
async fn rejected_delete_does_not_retry_or_switch_routes() {
    for status in [401, 403, 404] {
        let http = TestServer::start().await;
        get(
            &http,
            ResponseTemplate::new(200).set_body_json(comment("author", RAW)),
        )
        .await;
        Mock::given(method("DELETE"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let error = http
            .client()
            .delete_issue_comment("owner", "repo", 7)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing evidence")
        };
        assert_eq!(outcome.status, OutcomeStatus::NotApplied);
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn delete_does_not_succeed_for_still_present_or_unverifiable_comments() {
    for status in [200, 401, 403, 500] {
        let http = TestServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(ROUTE))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let response = if status == 200 {
            ResponseTemplate::new(200).set_body_json(comment("author", RAW))
        } else {
            ResponseTemplate::new(status)
        };
        deletion_reads(&http, 7, response, 200).await;
        let error = http
            .client()
            .delete_issue_comment("owner", "repo", 7)
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing evidence")
        };
        assert_eq!(
            outcome.status,
            if status == 200 {
                OutcomeStatus::NotVerified
            } else {
                OutcomeStatus::Unknown
            }
        );
    }
}

#[tokio::test]
async fn missing_comment_after_delete_is_unknown_when_repository_visibility_is_lost() {
    let http = TestServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    deletion_reads(&http, 7, ResponseTemplate::new(404), 404).await;
    let error = http
        .client()
        .delete_issue_comment("owner", "repo", 7)
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing evidence")
    };
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
}

#[tokio::test]
async fn invalid_ids_and_repository_segments_are_rejected_before_http() {
    let http = TestServer::start().await;
    let api = http.client();
    for id in [0, i32::MAX as u64 + 1, u64::MAX] {
        assert!(api.get_issue_comment("owner", "repo", id).await.is_err());
        assert!(api
            .update_issue_comment("owner", "repo", id, &UpdateComment { body: RAW.into() })
            .await
            .is_err());
        assert!(api.delete_issue_comment("owner", "repo", id).await.is_err());
    }
    assert!(api
        .delete_issue_comment("../owner", "repo", 7)
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
async fn single_get_rejects_a_different_comment_identity() {
    let http = TestServer::start().await;
    let mut value = comment("author", RAW);
    value["id"] = json!(8);
    get(&http, ResponseTemplate::new(200).set_body_json(value)).await;
    assert!(http
        .client()
        .get_issue_comment("owner", "repo", 7)
        .await
        .is_err());
}

#[tokio::test]
async fn patch_foreign_identity_retains_the_intended_and_observed_comment_ids() {
    let http = TestServer::start().await;
    let mut foreign = comment("editor", RAW);
    foreign["id"] = json!(8);
    Mock::given(method("PATCH"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_json(foreign))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let error = http
        .client()
        .update_issue_comment("owner", "repo", 7, &UpdateComment { body: RAW.into() })
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing evidence")
    };
    let target = outcome.target.unwrap();
    assert_eq!(target.number, Some(7));
    assert_eq!(target.observed_number, Some(8));
    assert_eq!(outcome.status, OutcomeStatus::Unknown);
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn mcp_update_and_delete_return_verified_observations() {
    let http = TestServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(ROUTE))
        .and(body_json(json!({"body":RAW})))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment("editor", RAW)))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let reads = std::sync::atomic::AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2 {
                ResponseTemplate::new(200).set_body_json(comment("author", RAW))
            } else {
                ResponseTemplate::new(404)
            }
        })
        .expect(3)
        .mount(&http.mock_server)
        .await;
    visible_repository(&http, 200).await;
    Mock::given(method("DELETE"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_string("7"))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let (a, b) = tokio::io::duplex(4096);
    let server = GitBucketMcpServer::new(http.client());
    tokio::spawn(async move {
        let service = server.serve(a).await.unwrap();
        let _ = service.waiting().await;
    });
    let client = ().serve(b).await.unwrap();
    let edited = client
        .call_tool(
            CallToolRequestParams::new("update_issue_comment").with_arguments(
                json!({"owner":" owner ","repo":" repo ","comment_id":7,"body":RAW})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(edited.is_error, Some(false));
    assert_eq!(edited.structured_content.unwrap(), comment("author", RAW));
    let deleted = client
        .call_tool(
            CallToolRequestParams::new("delete_issue_comment").with_arguments(
                json!({"owner":"owner","repo":"repo","comment_id":7})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(deleted.is_error, Some(false));
    assert_eq!(
        deleted.structured_content.unwrap(),
        json!({"deleted":true,"comment_id":7})
    );
    client.cancel().await.unwrap();
}
