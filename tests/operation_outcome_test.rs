#[allow(dead_code)]
mod common;

use gitbucket_mcp_server::{
    api::{client::GitBucketClient, http::HttpSettings, profile::ApiProfile},
    error::GbMcpError,
    models::{
        issue::UpdateIssue,
        label::UpdateLabel,
        milestone::{CreateMilestone, UpdateMilestone},
        operation::{FieldIntent, FieldStatus, OperationOutcome, OutcomeStatus, WriteRoute},
        pull_request::UpdatePullRequest,
    },
    server::GitBucketMcpServer,
    tools::response::from_gb_error,
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer) -> GitBucketClient {
    GitBucketClient::new_with_web_auth(
        &server.uri(),
        "fake-token",
        false,
        Some("alice"),
        Some("fake-password"),
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard)
}
async fn signin(server: &MockServer) {
    common::web_identity(server, "", "alice").await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=fake-session; Path=/"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Signed in"))
        .mount(server)
        .await;
}
fn outcome(error: &GbMcpError) -> &OperationOutcome {
    match error {
        GbMcpError::OperationFailed { outcome, .. } => outcome,
        error => panic!("expected operation outcome: {error}"),
    }
}
fn field(outcome: &OperationOutcome, name: &str) -> FieldStatus {
    outcome
        .fields
        .iter()
        .find(|field| field.field == name)
        .unwrap()
        .status
}
fn all_changes() -> UpdateIssue {
    UpdateIssue {
        title: Some("new title".into()),
        body: Some("new body".into()),
        state: Some("closed".into()),
    }
}
async fn issue_title(server: &MockServer) {
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(move |_: &wiremock::Request| {
            let title = if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                "old"
            } else {
                "new title"
            };
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":1,"title":title,"body":"old body","state":"open"}))
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(
            ResponseTemplate::new(303).insert_header("Location", "/owner/repo/issues/_data/1"),
        )
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/_data/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"title":"new title","content":"old body"})),
        )
        .mount(server)
        .await;
}
fn assert_issue_partial(operation: &OperationOutcome) {
    assert_eq!(operation.name, "update_issue");
    assert_eq!(operation.status, OutcomeStatus::Partial);
    assert_eq!(operation.route, Some(WriteRoute::Web));
    let target = operation.target.as_ref().unwrap();
    assert_eq!(target.number, Some(1));
    assert_eq!(target.owner.as_deref(), Some("owner"));
    assert_eq!(target.repo, "repo");
    assert_eq!(field(operation, "title"), FieldStatus::Verified);
    assert_eq!(field(operation, "body"), FieldStatus::Unknown);
    assert_eq!(field(operation, "state"), FieldStatus::NotAttempted);
}

#[tokio::test]
async fn issue_partial_error_keeps_verified_unknown_and_unattempted_fields_without_body_values() {
    let server = MockServer::start().await;
    signin(&server).await;
    issue_title(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit/1"))
        .respond_with(ResponseTemplate::new(500).set_body_string("fake-private-server-body"))
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .update_issue("owner", "repo", 1, &all_changes())
        .await
        .unwrap_err();
    assert_issue_partial(outcome(&error));
    let response = from_gb_error(error).unwrap();
    assert_eq!(response.is_error, Some(true));
    let payload = response.structured_content.unwrap();
    assert_eq!(payload["kind"], "operation_partial");
    // A web 500 is not a definite API rejection; only the safe HTTP summary is returned.
    assert_eq!(payload["cause"]["kind"], "other_error");
    let encoded = payload.to_string();
    for private in [
        "fake-private-server-body",
        "new title",
        "new body",
        "fake-token",
        "fake-password",
    ] {
        assert!(!encoded.contains(private), "{private}");
    }
    assert!(!server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path().ends_with("/state")));
}

#[tokio::test]
async fn rest_rejections_and_uncertain_writes_keep_distinct_outcomes_and_safe_causes() {
    for status in [400, 401, 403, 404, 409, 422, 408, 500] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/labels/old"))
            .respond_with(ResponseTemplate::new(status).set_body_string("fake-private-body"))
            .expect(1)
            .mount(&server)
            .await;
        let error = client(&server)
            .update_label(
                "owner",
                "repo",
                "old",
                &UpdateLabel {
                    new_name: Some("new".into()),
                    color: Some("abcdef".into()),
                },
            )
            .await
            .unwrap_err();
        let uncertain = status == 408 || status >= 500;
        assert_eq!(
            outcome(&error).status,
            if uncertain {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::NotApplied
            }
        );
        for name in ["name", "color"] {
            assert_eq!(
                field(outcome(&error), name),
                if uncertain {
                    FieldStatus::Unknown
                } else {
                    FieldStatus::Rejected
                }
            );
        }
        let result = from_gb_error(error).unwrap();
        let payload = result.structured_content.unwrap();
        assert_eq!(
            payload["kind"],
            if uncertain {
                "operation_unknown"
            } else {
                "operation_not_applied"
            }
        );
        assert_eq!(payload["status"], status);
        assert_eq!(payload["cause"]["kind"], "api_error");
        assert_eq!(payload["cause"]["status"], status);
        assert_eq!(payload["operation"]["target"]["name"], "old");
        assert_eq!(payload["operation"]["target"]["requested_name"], "new");
        assert_eq!(payload["operation"]["route"], "rest");
        assert!(!payload.to_string().contains("fake-private-body"));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn pr_partial_response_distinguishes_requested_fields_from_preserved_content() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":7,"title":"old","body":"keep","state":"open"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":7,"title":"new","body":"keep","state":"open"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .update_pull_request(
            "owner",
            "repo",
            7,
            &UpdatePullRequest {
                title: Some("new".into()),
                body: None,
                state: Some("closed".into()),
                base: None,
            },
        )
        .await
        .unwrap_err();
    let report = outcome(&error);
    assert_eq!(report.status, OutcomeStatus::Partial);
    assert_eq!(field(report, "title"), FieldStatus::Verified);
    assert_eq!(field(report, "body"), FieldStatus::Verified);
    assert_eq!(field(report, "state"), FieldStatus::Mismatched);
    assert_eq!(
        report
            .fields
            .iter()
            .find(|f| f.field == "body")
            .unwrap()
            .intent,
        FieldIntent::Preserved
    );
    assert_eq!(report.target.as_ref().unwrap().number, Some(7));
    assert_eq!(report.route, Some(WriteRoute::Rest));
}

#[tokio::test]
async fn wrong_write_response_identity_never_verifies_fields_from_another_pr() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":99,"title":"new","body":"new","state":"open"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .update_pull_request(
            "owner",
            "repo",
            7,
            &UpdatePullRequest {
                title: Some("new".into()),
                body: Some("new".into()),
                state: None,
                base: None,
            },
        )
        .await
        .unwrap_err();
    let report = outcome(&error);
    assert_eq!(report.status, OutcomeStatus::Unknown);
    assert_eq!(report.target.as_ref().unwrap().number, Some(7));
    assert_eq!(report.target.as_ref().unwrap().observed_number, Some(99));
    assert!(report
        .fields
        .iter()
        .all(|field| field.status == FieldStatus::Unknown));
}

#[tokio::test]
async fn milestone_content_is_verified_before_state_and_partial_progress_survives_failure() {
    for apply_content in [true, false] {
        let server = MockServer::start().await;
        signin(&server).await;
        let stored = Arc::new(Mutex::new(
            json!({"number":7,"title":"日本語","description":"old","state":"open"}),
        ));
        let read = stored.clone();
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/milestones/7"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200).set_body_json(read.lock().unwrap().clone())
            })
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/milestones/7/edit"))
            .respond_with(move |_: &wiremock::Request| {
                if apply_content {
                    stored.lock().unwrap()["description"] = json!("new");
                }
                ResponseTemplate::new(303)
                    .insert_header("Location", "/owner/repo/issues/milestones")
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/milestones"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<a href='/owner/repo/issues/milestones/new'>New</a>"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/milestones/7/close"))
            .respond_with(ResponseTemplate::new(500))
            .expect(if apply_content { 1 } else { 0 })
            .mount(&server)
            .await;
        let error = client(&server)
            .update_milestone(
                "owner",
                "repo",
                7,
                &UpdateMilestone {
                    title: None,
                    description: Some("new".into()),
                    due_on: None,
                    state: Some("closed".into()),
                },
            )
            .await
            .unwrap_err();
        let report = outcome(&error);
        assert_eq!(
            report.status,
            if apply_content {
                OutcomeStatus::Partial
            } else {
                OutcomeStatus::NotVerified
            }
        );
        assert_eq!(
            field(report, "description"),
            if apply_content {
                FieldStatus::Verified
            } else {
                FieldStatus::Mismatched
            }
        );
        assert_eq!(
            field(report, "state"),
            if apply_content {
                FieldStatus::Unknown
            } else {
                FieldStatus::NotAttempted
            }
        );
        assert_eq!(report.target.as_ref().unwrap().number, Some(7));
    }
}

#[tokio::test]
async fn ambiguous_milestone_creation_exposes_candidates_without_inventing_a_created_id() {
    let server = MockServer::start().await;
    signin(&server).await;
    let reads = AtomicUsize::new(0);
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/milestones"))
        .respond_with(move |_: &wiremock::Request| ResponseTemplate::new(200).set_body_json(
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {json!([])} else {json!([{"number":7,"title":"日本語","state":"open"},{"number":8,"title":"日本語","state":"open"}])}
        )).expect(2).mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/new"))
        .respond_with(
            ResponseTemplate::new(303).insert_header("Location", "/owner/repo/issues/milestones"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/milestones"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<a href='/owner/repo/issues/milestones/new'>New</a>"),
        )
        .mount(&server)
        .await;
    let error = client(&server)
        .create_milestone(
            "owner",
            "repo",
            &CreateMilestone {
                title: "日本語".into(),
                description: None,
                due_on: None,
            },
        )
        .await
        .unwrap_err();
    let report = outcome(&error);
    assert_eq!(report.status, OutcomeStatus::Unknown);
    assert_eq!(report.target.as_ref().unwrap().number, None);
    assert_eq!(report.target.as_ref().unwrap().candidate_numbers, [7, 8]);
    assert!(report
        .fields
        .iter()
        .all(|field| field.status == FieldStatus::Unknown));
}

#[tokio::test]
async fn tool_deadline_and_cancellation_keep_prior_field_evidence_and_stop_later_writes() {
    use rmcp::{service::RequestContext, ServerHandler};
    for cancel in [false, true] {
        let http = MockServer::start().await;
        signin(&http).await;
        issue_title(&http).await;
        let arrived = Arc::new(tokio::sync::Notify::new());
        let notify = arrived.clone();
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/edit/1"))
            .respond_with(move |_: &wiremock::Request| {
                notify.notify_one();
                ResponseTemplate::new(200).set_delay(Duration::from_secs(3))
            })
            .expect(1)
            .mount(&http)
            .await;
        let api = GitBucketClient::new_with_web_auth_and_http_settings(
            &http.uri(),
            "fake-token",
            false,
            Some("alice"),
            Some("fake-password"),
            HttpSettings {
                request_timeout: Duration::from_secs(5),
                tool_timeout: if cancel {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(500)
                },
                ..Default::default()
            },
        )
        .unwrap()
        .with_api_profile(ApiProfile::Standard);
        let server = GitBucketMcpServer::new(api);
        let server_clone = server.clone();
        let (server_transport, client_transport) = tokio::io::duplex(4096);
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let running = server_clone.serve(server_transport).await.unwrap();
            let _ = tx.send(running.peer().clone());
            let _ = running.waiting().await;
        });
        let peer = ().serve(client_transport).await.unwrap();
        let context = RequestContext::new(rmcp::model::RequestId::Number(123), rx.await.unwrap());
        let ct = context.ct.clone();
        let params = CallToolRequestParams::new("update_issue").with_arguments(json!({"owner":"owner","repo":"repo","issue_number":1,"title":"new title","body":"new body","state":"closed"}).as_object().unwrap().clone());
        let task = tokio::spawn(async move { server.call_tool(params, context).await.unwrap() });
        tokio::time::timeout(Duration::from_secs(2), arrived.notified())
            .await
            .unwrap();
        if cancel {
            ct.cancel();
        }
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["kind"], "operation_partial");
        assert_eq!(
            payload["cause"]["kind"],
            if cancel {
                "cancelled_error"
            } else {
                "timeout_error"
            }
        );
        let report: OperationOutcome =
            serde_json::from_value(payload["operation"].clone()).unwrap();
        assert_issue_partial(&report);
        assert!(!http
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path().ends_with("/state")));
        peer.cancel().await.unwrap();
    }
}
