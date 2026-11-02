#[allow(dead_code)]
mod common;

use common::TestServer;
use gitbucket_mcp_server::models::milestone::{CreateMilestone, UpdateMilestone};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn change() -> UpdateMilestone {
    UpdateMilestone {
        title: None,
        state: None,
        description: None,
        due_on: None,
    }
}

// Official 4.44.0/4.48.0 use CreateAMilestone for PATCH: title is required,
// state defaults to open, and omitted description/due_on are replaced by None.
async fn mount_contract(server: &TestServer) -> Arc<Mutex<Value>> {
    let current = Arc::new(Mutex::new(json!({
        "number":7,"title":"v1.0","state":"closed",
        "description":"Keep this description","due_on":"2026-04-01T00:00:00Z"
    })));
    let read = Arc::clone(&current);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(read.lock().unwrap().clone())
        })
        .mount(&server.mock_server)
        .await;
    let write = Arc::clone(&current);
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(move |r: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            let Some(title) = body.get("title") else {
                return ResponseTemplate::new(404);
            };
            let mut value = write.lock().unwrap();
            value["title"] = title.clone();
            value["state"] = body.get("state").cloned().unwrap_or(json!("open"));
            value["description"] = body.get("description").cloned().unwrap_or_default();
            value["due_on"] = body.get("due_on").cloned().unwrap_or_default();
            ResponseTemplate::new(200).set_body_json(value.clone())
        })
        .expect(1)
        .mount(&server.mock_server)
        .await;
    current
}

#[tokio::test]
async fn test_title_only_preserves_closed_state_description_and_due_date() {
    let server = TestServer::start().await;
    mount_contract(&server).await;
    let mut update = change();
    update.title = Some("v1.1".into());
    let actual = server
        .client()
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(actual.title, "v1.1");
    assert_eq!(actual.state, "closed");
    assert_eq!(actual.description.as_deref(), Some("Keep this description"));
    assert_eq!(actual.due_on.as_deref(), Some("2026-04-01T00:00:00Z"));
}

#[tokio::test]
async fn test_state_only_uses_rest_without_web_credentials() {
    let server = TestServer::start().await;
    mount_contract(&server).await;
    let mut update = change();
    update.state = Some("open".into());
    let actual = server
        .client()
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(actual.state, "open");
    assert_eq!(actual.title, "v1.0");
    assert_eq!(actual.description.as_deref(), Some("Keep this description"));
    assert_eq!(actual.due_on.as_deref(), Some("2026-04-01T00:00:00Z"));
}

#[tokio::test]
async fn test_description_only_preserves_state_and_due_date() {
    let server = TestServer::start().await;
    mount_contract(&server).await;
    let mut update = change();
    update.description = Some("Changed notes".into());
    let actual = server
        .client()
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(actual.description.as_deref(), Some("Changed notes"));
    assert_eq!(actual.state, "closed");
    assert_eq!(actual.due_on.as_deref(), Some("2026-04-01T00:00:00Z"));
}

#[tokio::test]
async fn test_clear_date_and_description_sends_explicit_null() {
    let server = TestServer::start().await;
    mount_contract(&server).await;
    let mut update = change();
    update.due_on = Some(String::new());
    update.description = Some(String::new());
    let actual = server
        .client()
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert!(actual.due_on.is_none());
    assert!(actual.description.as_deref().unwrap_or_default().is_empty());
    assert_eq!(actual.state, "closed");
    let requests = server.mock_server.received_requests().await.unwrap();
    let patch = requests.iter().find(|r| r.method == "PATCH").unwrap();
    let body: Value = serde_json::from_slice(&patch.body).unwrap();
    assert!(body.get("due_on").unwrap().is_null());
}

#[tokio::test]
async fn test_date_only_and_offset_dates_are_normalized_for_rest() {
    for (input, expected) in [
        ("2028-02-29", "2028-02-29T00:00:00Z"),
        ("2026-04-01T00:30:00+09:00", "2026-03-31T15:30:00Z"),
    ] {
        let server = TestServer::start().await;
        mount_contract(&server).await;
        let mut update = change();
        update.due_on = Some(input.into());
        let actual = server
            .client()
            .update_milestone("owner", "repo", 7, &update)
            .await
            .unwrap();
        assert_eq!(actual.due_on.as_deref(), Some(expected));
        assert_eq!(actual.description.as_deref(), Some("Keep this description"));
        assert_eq!(actual.state, "closed");
    }
}

#[tokio::test]
async fn test_invalid_dates_are_rejected_before_any_request() {
    for input in [
        "2026-あいう",
        "2026-02-29",
        "2026-13-01",
        "2026-04-01Tgarbage",
        "tomorrow",
    ] {
        let server = TestServer::start().await;
        let mut update = change();
        update.due_on = Some(input.into());
        assert!(server
            .client()
            .update_milestone("owner", "repo", 7, &update)
            .await
            .is_err());
        assert!(
            server
                .mock_server
                .received_requests()
                .await
                .unwrap()
                .is_empty(),
            "{input}"
        );
        let create = CreateMilestone {
            title: "v1.0".into(),
            description: None,
            due_on: Some(input.into()),
        };
        assert!(server
            .client()
            .create_milestone("owner", "repo", &create)
            .await
            .is_err());
        assert!(
            server
                .mock_server
                .received_requests()
                .await
                .unwrap()
                .is_empty(),
            "{input}"
        );
    }
}

#[tokio::test]
async fn test_create_normalizes_date_before_post() {
    let server = TestServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/milestones"))
        .respond_with(|r: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body["due_on"], "2026-04-01T00:00:00Z");
            let mut value = body;
            value["number"] = json!(7);
            ResponseTemplate::new(201).set_body_json(value)
        })
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let create = CreateMilestone {
        title: "v1.0".into(),
        description: None,
        due_on: Some("2026-04-01".into()),
    };
    assert_eq!(
        server
            .client()
            .create_milestone("owner", "repo", &create)
            .await
            .unwrap()
            .number,
        7
    );
}

#[tokio::test]
async fn test_ignored_and_unreadable_rest_responses_do_not_trigger_retries() {
    for unreadable in [false, true] {
        let server = TestServer::start().await;
        let value = json!({"number":7,"title":"v1.0","state":"closed"});
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/milestones/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(value.clone()))
            .expect(1)
            .mount(&server.mock_server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/milestones/7"))
            .respond_with(if unreadable {
                ResponseTemplate::new(200).set_body_string("invalid json")
            } else {
                ResponseTemplate::new(200).set_body_json(value)
            })
            .expect(1)
            .mount(&server.mock_server)
            .await;
        let mut update = change();
        update.state = Some("open".into());
        let err = server
            .client()
            .update_milestone("owner", "repo", 7, &update)
            .await
            .unwrap_err();
        if unreadable {
            assert!(matches!(
                err,
                gitbucket_mcp_server::error::GbMcpError::OperationOutcomeUnknown { .. }
            ));
        } else {
            assert!(matches!(
                err,
                gitbucket_mcp_server::error::GbMcpError::OperationNotVerified { .. }
            ));
        }
        assert_eq!(
            server.mock_server.received_requests().await.unwrap().len(),
            2
        );
    }
}

async fn web_signin(server: &TestServer) {
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("location", "/dashboard")
                .insert_header("set-cookie", "JSESSIONID=test-session; Path=/"),
        )
        .mount(&server.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dashboard"))
        .respond_with(ResponseTemplate::new(200).set_body_string("signed in"))
        .mount(&server.mock_server)
        .await;
}

async fn web_current(server: &TestServer, date: &str) -> Arc<Mutex<Value>> {
    let current = Arc::new(Mutex::new(
        json!({"number":7,"title":"日本語","state":"closed","description":"Keep notes","due_on":date}),
    ));
    let read = Arc::clone(&current);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(read.lock().unwrap().clone())
        })
        .mount(&server.mock_server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    current
}

#[tokio::test]
async fn test_web_content_update_rejects_time_loss_before_signin() {
    let server = TestServer::start().await;
    web_current(&server, "2026-04-01T12:00:00Z").await;
    let mut update = change();
    update.title = Some("別の日本語".into());
    let err = server
        .legacy_client_with_web_auth("alice", "fake-password")
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        gitbucket_mcp_server::error::GbMcpError::Unsupported(_)
    ));
    let requests = server.mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.method != "POST"));
}

#[tokio::test]
async fn test_web_state_only_preserves_due_time_and_skips_content_form() {
    let server = TestServer::start().await;
    let current = web_current(&server, "2026-04-01T12:00:00Z").await;
    web_signin(&server).await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/milestones/7/open"))
        .respond_with(move |_: &wiremock::Request| {
            current.lock().unwrap()["state"] = json!("open");
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/7/edit"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server.mock_server)
        .await;
    let mut update = change();
    update.state = Some("open".into());
    let actual = server
        .legacy_client_with_web_auth("alice", "fake-password")
        .update_milestone("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(actual.state, "open");
    assert_eq!(actual.due_on.as_deref(), Some("2026-04-01T12:00:00Z"));
}

#[tokio::test]
async fn test_web_200_validation_page_is_not_success() {
    let server = TestServer::start().await;
    web_current(&server, "2026-04-01T00:00:00Z").await;
    web_signin(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/7/edit"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Validation failed</html>"))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = change();
    update.description = Some("Changed notes".into());
    assert!(
        matches!(server.legacy_client_with_web_auth("alice","fake-password").update_milestone("owner","repo",7,&update).await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationNotVerified{fields,..}) if fields==["description"])
    );
}

#[tokio::test]
async fn test_web_creation_never_returns_an_existing_same_title_milestone() {
    let server = TestServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/milestones"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name":"repo","full_name":"owner/repo","private":false})),
        )
        .mount(&server.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones"))
        .and(query_param("state", "all"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([{"number":7,"title":"日本語","state":"open"}])),
        )
        .expect(2)
        .mount(&server.mock_server)
        .await;
    web_signin(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Milestone already exists"))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let body = CreateMilestone {
        title: "日本語".into(),
        description: None,
        due_on: None,
    };
    assert!(matches!(
        server
            .legacy_client_with_web_auth("alice", "fake-password")
            .create_milestone("owner", "repo", &body)
            .await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationOutcomeUnknown { .. })
    ));
}

#[tokio::test]
async fn test_noop_update_does_not_send_a_write() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":7,"title":"v1.0","state":"closed"})),
        )
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = change();
    update.state = Some("closed".into());
    assert_eq!(
        server
            .client()
            .update_milestone("owner", "repo", 7, &update)
            .await
            .unwrap()
            .state,
        "closed"
    );
    assert_eq!(
        server.mock_server.received_requests().await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn test_snapshot_read_failure_prevents_patch() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = change();
    update.title = Some("v1.1".into());
    assert!(matches!(
        server
            .client()
            .update_milestone("owner", "repo", 7, &update)
            .await,
        Err(gitbucket_mcp_server::error::GbMcpError::Api { status: 403, .. })
    ));
    assert_eq!(
        server.mock_server.received_requests().await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn test_write_rejections_and_uncertain_errors_do_not_fall_back() {
    for status in [403, 422, 408, 500] {
        let server = TestServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/milestones/7"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"number":7,"title":"v1.0","state":"closed"})),
            )
            .expect(1)
            .mount(&server.mock_server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/milestones/7"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server.mock_server)
            .await;
        let mut update = change();
        update.state = Some("open".into());
        let err = server
            .legacy_client_with_web_auth("alice", "fake-password")
            .update_milestone("owner", "repo", 7, &update)
            .await
            .unwrap_err();
        if status == 403 || status == 422 {
            assert!(
                matches!(err,gitbucket_mcp_server::error::GbMcpError::Api{status:actual,..} if actual==status)
            );
        } else {
            assert!(matches!(
                err,
                gitbucket_mcp_server::error::GbMcpError::OperationOutcomeUnknown { .. }
            ));
        }
        assert_eq!(
            server.mock_server.received_requests().await.unwrap().len(),
            2
        );
    }
}

#[tokio::test]
async fn test_web_postwrite_read_failure_is_unknown() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let server = TestServer::start().await;
    let reads = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(move |_: &wiremock::Request| {
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"number":7,"title":"日本語","state":"open"}))
            } else {
                ResponseTemplate::new(403)
            }
        })
        .expect(2)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/milestones/7"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    web_signin(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/7/edit"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = change();
    update.description = Some("New notes".into());
    assert!(matches!(
        server
            .legacy_client_with_web_auth("alice", "fake-password")
            .update_milestone("owner", "repo", 7, &update)
            .await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationOutcomeUnknown { .. })
    ));
}
