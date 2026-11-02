use gitbucket_mcp_server::{
    api::{
        client::GitBucketClient,
        compatibility::{Compatibility, Operation, Route},
    },
    error::GbMcpError,
    models::{
        issue::UpdateIssue,
        label::{CreateLabel, UpdateLabel},
        milestone::CreateMilestone,
        pull_request::UpdatePullRequest,
    },
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer, mode: Compatibility) -> GitBucketClient {
    GitBucketClient::new_with_web_auth(
        &server.uri(),
        "fake-token",
        false,
        Some("user"),
        Some("fake-password"),
    )
    .unwrap()
    .with_compatibility(mode)
}
fn rename() -> UpdateLabel {
    UpdateLabel {
        new_name: Some("new-name".into()),
        color: Some("ABCDEF".into()),
        description: None,
    }
}
fn recolor() -> UpdateLabel {
    UpdateLabel {
        new_name: None,
        color: Some("ABCDEF".into()),
        description: None,
    }
}
fn pr_update() -> UpdatePullRequest {
    UpdatePullRequest {
        title: None,
        body: None,
        state: Some("closed".into()),
        base: None,
    }
}
fn issue_update() -> UpdateIssue {
    UpdateIssue {
        title: Some("new title".into()),
        body: None,
        state: None,
    }
}
async fn current_label(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/labels/old-name"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":"123456"})),
        )
        .mount(server)
        .await;
}
async fn sign_in(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/home"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/home"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
}

#[test]
fn profiles_express_operations_and_evidence_without_guessing_version() {
    for mode in [Compatibility::GitBucket444, Compatibility::GitBucket448] {
        for op in [
            Operation::IssueUpdate,
            Operation::LabelSameNameUpdate,
            Operation::MilestoneRestrictedTitle,
        ] {
            assert_eq!(mode.capability(op).route, Route::Web);
        }
        for op in [
            Operation::PullRequestUpdate,
            Operation::LabelRename,
            Operation::MilestoneWrite,
            Operation::MilestoneDelete,
        ] {
            assert_eq!(mode.capability(op).route, Route::Rest);
        }
        assert!(mode
            .capability(Operation::IssueUpdate)
            .evidence
            .contains("Operator-selected"));
    }
    assert_eq!(Compatibility::default(), Compatibility::RestOnly);
    assert_eq!(
        Compatibility::RestOnly
            .capability(Operation::IssueUpdate)
            .route,
        Route::Unverified
    );
    assert_eq!(
        Compatibility::Legacy404
            .capability(Operation::LabelDescription)
            .route,
        Route::Unsupported
    );
    for mode in [
        "rest-only",
        "gitbucket-4.44.0",
        "gitbucket-4.48.0",
        "legacy-404",
    ] {
        assert!(mode.parse::<Compatibility>().is_ok());
    }
    assert!(!"fake-secret-bad-mode"
        .parse::<Compatibility>()
        .unwrap_err()
        .to_string()
        .contains("fake-secret"));
}

#[tokio::test]
async fn unknown_404_preserves_error_without_preflight_or_web_even_with_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let client = client(&server, Compatibility::RestOnly);
    let results = [
        client
            .update_pull_request("owner", "repo", 1, &pr_update())
            .await
            .unwrap_err(),
        client
            .update_issue("owner", "repo", 1, &issue_update())
            .await
            .unwrap_err(),
        client
            .update_label("owner", "repo", "old-name", &rename())
            .await
            .unwrap_err(),
        client
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
            .unwrap_err(),
        client
            .delete_milestone("owner", "repo", 1)
            .await
            .unwrap_err(),
    ];
    for result in results {
        assert!(matches!(result, GbMcpError::Api { status: 404, .. }));
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    assert!(requests
        .iter()
        .all(|r| r.url.path().starts_with("/api/v3/")));
}

#[tokio::test]
async fn standard_profiles_keep_rest_for_pr_rename_and_normal_milestone_errors() {
    for mode in [Compatibility::GitBucket444, Compatibility::GitBucket448] {
        for status in [401, 403, 404, 409, 422] {
            let server = MockServer::start().await;
            Mock::given(method("PATCH"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let c = client(&server, mode);
            for error in [
                c.update_pull_request("owner", "repo", 1, &pr_update())
                    .await
                    .unwrap_err(),
                c.update_label("owner", "repo", "old-name", &rename())
                    .await
                    .unwrap_err(),
                c.create_milestone(
                    "owner",
                    "repo",
                    &CreateMilestone {
                        title: "title+1".into(),
                        description: None,
                        due_on: None,
                    },
                )
                .await
                .unwrap_err(),
            ] {
                assert!(matches!(error,GbMcpError::Api{status:s,..} if s==status));
            }
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 3);
            assert!(requests
                .iter()
                .all(|r| r.url.path().starts_with("/api/v3/")));
        }
    }
}

#[tokio::test]
async fn label_rename_stays_rest_and_ignored_responses_are_not_success() {
    for mode in [
        Compatibility::RestOnly,
        Compatibility::GitBucket444,
        Compatibility::GitBucket448,
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/labels/old-name"))
            .and(body_json(json!({"name":"new-name","color":"abcdef"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"name":"new-name","color":"abcdef"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            client(&server, mode)
                .update_label("owner", "repo", "old-name", &rename())
                .await
                .unwrap()
                .name,
            "new-name"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server, Compatibility::GitBucket444)
            .update_label("owner", "repo", "old-name", &rename())
            .await,
        Err(GbMcpError::OperationNotVerified { .. })
    ));
}

#[tokio::test]
async fn same_name_color_uses_web_only_for_selected_contract_and_verifies_result() {
    for mode in [Compatibility::GitBucket444, Compatibility::GitBucket448] {
        for applies in [true, false] {
            let server = MockServer::start().await;
            let changed = Arc::new(AtomicBool::new(false));
            let read = changed.clone();
            Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/labels/old-name"))
                .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"name":"old-name","color":if read.load(Ordering::SeqCst){"abcdef"}else{"123456"}}))).mount(&server).await;
            sign_in(&server).await;
            Mock::given(method("GET")).and(path("/owner/repo/issues/labels"))
                .respond_with(ResponseTemplate::new(200).set_body_string(r#"<tr id="label-row-12"><a href="/owner/repo/issues?labels=old-name">old-name</a></tr>"#)).mount(&server).await;
            Mock::given(method("POST"))
                .and(path("/owner/repo/issues/labels/12/edit"))
                .respond_with(move |_: &wiremock::Request| {
                    changed.store(applies, Ordering::SeqCst);
                    ResponseTemplate::new(200).set_body_string("form page")
                })
                .expect(1)
                .mount(&server)
                .await;
            let result = client(&server, mode)
                .update_label("owner", "repo", "old-name", &recolor())
                .await;
            if applies {
                assert_eq!(result.unwrap().color.as_deref(), Some("abcdef"));
            } else {
                assert!(matches!(
                    result,
                    Err(GbMcpError::OperationNotVerified { .. })
                ));
            }
            assert!(!server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.method.as_str() == "PATCH"));
        }
    }
}

#[tokio::test]
async fn unverified_color_422_is_not_a_reason_to_try_web() {
    for mode in [Compatibility::RestOnly, Compatibility::Legacy404] {
        let server = MockServer::start().await;
        current_label(&server).await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(422))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            client(&server, mode)
                .update_label("owner", "repo", "old-name", &recolor())
                .await,
            Err(GbMcpError::Api { status: 422, .. })
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn description_including_clear_or_combined_change_is_rejected_before_any_request() {
    let server = MockServer::start().await;
    for mode in [
        Compatibility::RestOnly,
        Compatibility::GitBucket444,
        Compatibility::GitBucket448,
        Compatibility::Legacy404,
    ] {
        let c = client(&server, mode);
        for description in ["", "ignored"] {
            let create = CreateLabel {
                name: "label".into(),
                color: "123456".into(),
                description: Some(description.into()),
            };
            assert!(matches!(
                c.create_label("owner", "repo", &create).await,
                Err(GbMcpError::Unsupported(_))
            ));
            for mut change in [
                rename(),
                recolor(),
                UpdateLabel {
                    new_name: None,
                    color: None,
                    description: None,
                },
            ] {
                change.description = Some(description.into());
                let error = c
                    .update_label("owner", "repo", "old-name", &change)
                    .await
                    .unwrap_err();
                let result = gitbucket_mcp_server::tools::response::from_gb_error(error).unwrap();
                assert_eq!(
                    result.structured_content.unwrap()["kind"],
                    "unsupported_error"
                );
            }
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn normalized_color_noop_and_missing_web_credentials_do_not_write() {
    let server = MockServer::start().await;
    current_label(&server).await;
    let c = GitBucketClient::new(&server.uri(), "fake-token")
        .unwrap()
        .with_compatibility(Compatibility::GitBucket444);
    let noop = UpdateLabel {
        new_name: Some("old-name".into()),
        color: Some("#123456".into()),
        description: None,
    };
    assert_eq!(
        c.update_label("owner", "repo", "old-name", &noop)
            .await
            .unwrap()
            .color
            .as_deref(),
        Some("123456")
    );
    assert!(c
        .update_label("owner", "repo", "old-name", &recolor())
        .await
        .unwrap_err()
        .to_string()
        .contains("GITBUCKET_USERNAME"));
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method.as_str() == "GET"));
}

#[tokio::test]
async fn known_issue_updates_use_web_directly_without_patch_or_dummy_comment() {
    let server = MockServer::start().await;
    let changed = Arc::new(AtomicBool::new(false));
    let read = changed.clone();
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"number":1,"title":if read.load(Ordering::SeqCst){"new title"}else{"old"},"state":"open"}))).mount(&server).await;
    sign_in(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(move |_: &wiremock::Request| {
            changed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server, Compatibility::GitBucket444)
            .update_issue("owner", "repo", 1, &issue_update())
            .await
            .unwrap()
            .title,
        "new title"
    );
    let requests = server.received_requests().await.unwrap();
    assert!(!requests
        .iter()
        .any(|r| r.method.as_str() == "PATCH" || r.url.path().ends_with("/comments")));
}

#[tokio::test]
async fn known_restricted_milestone_titles_select_web_before_rest_write() {
    let server = MockServer::start().await;
    let changed = Arc::new(AtomicBool::new(false));
    let read = changed.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/milestones"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(if read.load(Ordering::SeqCst) {
                json!([{"number":1,"title":"日本語","state":"open"}])
            } else {
                json!([])
            })
        })
        .mount(&server)
        .await;
    sign_in(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/milestones/new"))
        .respond_with(move |_: &wiremock::Request| {
            changed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server, Compatibility::GitBucket448)
            .create_milestone(
                "owner",
                "repo",
                &CreateMilestone {
                    title: "日本語".into(),
                    description: None,
                    due_on: None
                }
            )
            .await
            .unwrap()
            .number,
        1
    );
    assert!(!server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.method.as_str() == "POST" && r.url.path().starts_with("/api/v3/")));
}

#[tokio::test]
async fn explicit_legacy_mode_does_not_fallback_from_rejections_or_unknown_outcomes() {
    for status in [401, 403, 409, 422, 408, 500] {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let c = client(&server, Compatibility::Legacy404);
        let results = [
            c.update_issue("owner", "repo", 1, &issue_update())
                .await
                .unwrap_err(),
            c.update_pull_request("owner", "repo", 1, &pr_update())
                .await
                .unwrap_err(),
            c.update_label("owner", "repo", "old-name", &rename())
                .await
                .unwrap_err(),
        ];
        for error in results {
            if status == 408 || status == 500 {
                assert!(matches!(
                    error,
                    GbMcpError::OperationOutcomeUnknown { .. }
                        | GbMcpError::WriteOutcomeUnknown { .. }
                ));
            } else {
                assert!(matches!(error,GbMcpError::Api{status:s,..} if s==status));
            }
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|r| r.method.as_str() == "PATCH"));
    }
}

#[tokio::test]
async fn a_wrong_label_snapshot_is_not_used_for_a_web_write() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/labels/old-name"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name":"another-label","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(client(&server, Compatibility::GitBucket444)
        .update_label("owner", "repo", "old-name", &recolor())
        .await
        .is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn label_creation_verifies_name_and_color_instead_of_assuming_http_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"name":"ignored-name","color":"123456"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server, Compatibility::RestOnly)
            .create_label(
                "owner",
                "repo",
                &CreateLabel {
                    name: "label".into(),
                    color: "abcdef".into(),
                    description: None
                }
            )
            .await,
        Err(GbMcpError::OperationNotVerified { .. })
    ));
}
