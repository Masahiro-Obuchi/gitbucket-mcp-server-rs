use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::{
        issue::UpdateIssue,
        milestone::{CreateMilestone, UpdateMilestone},
    },
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer, username: &str) -> GitBucketClient {
    client_at(server, "", username)
}
fn client_at(server: &MockServer, prefix: &str, username: &str) -> GitBucketClient {
    GitBucketClient::new_with_web_auth(
        &format!("{}{prefix}/api/v3", server.uri()),
        "fake-pat",
        false,
        Some(username),
        Some("fake-password"),
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard)
}
async fn auth(server: &MockServer, pat: &str, cookie: &str) {
    auth_at(server, "", pat, cookie).await;
}
async fn auth_at(server: &MockServer, prefix: &str, pat: &str, cookie: &str) {
    let (pat, cookie) = (pat.to_string(), cookie.to_string());
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/api/v3/user")))
        .respond_with(move |request: &wiremock::Request| {
            if request.headers.contains_key("authorization") {
                ResponseTemplate::new(200).set_body_json(json!({"login":pat}))
            } else if request
                .headers
                .get("cookie")
                .is_some_and(|v| v.to_str().unwrap().contains("JSESSIONID="))
            {
                ResponseTemplate::new(200).set_body_json(json!({"login":cookie}))
            } else {
                ResponseTemplate::new(401)
            }
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{prefix}/signin")))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", format!("{prefix}/;jsessionid=fake-url-session"))
                .insert_header("Set-Cookie", "JSESSIONID=fake-cookie; Path=/"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html>Authenticated landing</html>"),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn ordinary_forms_in_auth_named_repositories_and_contexts_are_accepted() {
    for (prefix, owner, repo) in [
        ("", "owner", "oauth-tools"),
        ("/oidc-context", "owner", "repo"),
        ("/oauth-context", "owner", "repo"),
        ("/saml-context", "owner", "repo"),
        ("/team/api/v3", "oauth", "repo"),
    ] {
        let server = MockServer::start().await;
        auth_at(&server, prefix, "alice", "alice").await;
        let session = client_at(&server, prefix, "alice")
            .web_session()
            .await
            .unwrap();
        let issues = format!("{prefix}/{owner}/{repo}/issues");
        let form = format!(
            "<form action='{}{issues}/milestones/new'>New milestone</form>",
            server.uri()
        );
        let label_page = format!(
            "{form}<table><tr id='label-row-12'><td><a href='{issues}?labels=bug'>bug</a></td></tr></table>"
        );
        Mock::given(method("GET"))
            .and(path(format!("{issues}/labels")))
            .respond_with(ResponseTemplate::new(200).set_body_string(label_page.clone()))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            session.find_label_id(owner, repo, "bug").await.unwrap(),
            12,
            "{prefix}/{owner}/{repo}"
        );
        Mock::given(method("POST"))
            .and(path(format!("{issues}/labels/12/edit")))
            .respond_with(ResponseTemplate::new(200).set_body_string(label_page))
            .expect(1)
            .mount(&server)
            .await;
        session
            .edit_label(owner, repo, 12, "bug", "abcdef")
            .await
            .unwrap();
        Mock::given(method("POST"))
            .and(path(format!("{issues}/milestones/new")))
            .respond_with(
                ResponseTemplate::new(303)
                    .insert_header("Location", format!("{issues}/milestones")),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{issues}/milestones")))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                "{form}<a href='{issues}/milestones/new'>New milestone</a>"
            )))
            .expect(1)
            .mount(&server)
            .await;
        session
            .create_milestone(owner, repo, "日本語", None, None)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn authentication_forms_on_resource_responses_remain_errors() {
    for operation in ["label-read", "label-write", "milestone", "issue-state"] {
        let server = MockServer::start().await;
        auth(&server, "alice", "alice").await;
        let session = client(&server, "alice").web_session().await.unwrap();
        let auth_form = "<form action='/signin/oidc'>Sign in with SSO</form>";
        let label_page = format!(
            "{auth_form}<table><tr id='label-row-12'><td><a href='/owner/repo/issues?labels=bug'>bug</a></td></tr></table>"
        );
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_string(label_page.clone()))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/labels/12/edit"))
            .respond_with(ResponseTemplate::new(200).set_body_string(label_page))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/milestones/new"))
            .respond_with(
                ResponseTemplate::new(303)
                    .insert_header("Location", "/owner/repo/issues/milestones"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/milestones"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                "{auth_form}<a href='/owner/repo/issues/milestones/new'>New</a>"
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/owner/repo/issue_comments/state"))
            .respond_with(
                ResponseTemplate::new(303).insert_header("Location", "/owner/repo/issues/1"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/owner/repo/issues/1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(auth_form))
            .mount(&server)
            .await;
        let error = match operation {
            "label-read" => session
                .find_label_id("owner", "repo", "bug")
                .await
                .map(|_| ()),
            "label-write" => {
                session
                    .edit_label("owner", "repo", 12, "bug", "abcdef")
                    .await
            }
            "milestone" => {
                session
                    .create_milestone("owner", "repo", "日本語", None, None)
                    .await
            }
            "issue-state" => {
                session
                    .update_issue_state("owner", "repo", 1, "close")
                    .await
            }
            _ => unreachable!(),
        }
        .unwrap_err();
        if operation == "label-read" {
            assert!(matches!(error, GbMcpError::Authentication(_)));
        } else {
            assert!(
                matches!(error, GbMcpError::OperationOutcomeUnknown { cause, .. } if matches!(*cause, GbMcpError::Authentication(_)))
            );
        }
    }
}

#[tokio::test]
async fn invalid_label_lookup_targets_are_rejected_before_identity_requests() {
    let server = MockServer::start().await;
    auth(&server, "alice", "alice").await;
    let session = client(&server, "alice").web_session().await.unwrap();
    // A stale session must not mask the local target-validation failure.
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(1)
        .mount(&server)
        .await;
    let before = server.received_requests().await.unwrap().len();
    for (owner, repo) in [("owner", "repo#other"), ("../owner", "repo"), ("owner", "")] {
        assert!(matches!(
            session.find_label_id(owner, repo, "bug").await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), before);
}
#[tokio::test]
async fn configured_account_mismatch_is_rejected_before_password_submission() {
    let server = MockServer::start().await;
    auth(&server, "alice", "bob").await;
    let error = client(&server, "bob").web_session().await.unwrap_err();
    assert!(matches!(error, GbMcpError::Authentication(_)));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
#[tokio::test]
async fn real_cookie_account_must_match_pat_and_session_urls_are_not_reused() {
    let server = MockServer::start().await;
    auth(&server, "alice", "bob").await;
    assert!(matches!(
        client(&server, "alice").web_session().await,
        Err(GbMcpError::Authentication(_))
    ));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests
        .iter()
        .all(|r| !r.url.as_str().contains("fake-url-session")));
    assert!(requests
        .iter()
        .filter(|r| r.headers.contains_key("authorization"))
        .all(|r| r.url.path() == "/api/v3/user"));
    assert!(requests.last().unwrap().headers.contains_key("cookie"));
    assert!(!requests
        .last()
        .unwrap()
        .headers
        .contains_key("authorization"));
}
#[tokio::test]
async fn anonymous_or_sso_200_login_pages_are_not_authenticated_sessions() {
    for body in [
        "<form action='/signin'><input type='password' value='fake-secret'></form>",
        "<form action='/signin/oidc'>Sign in with SSO</form>",
        "<html>Public home</html>",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(|r: &wiremock::Request| {
                if r.headers.contains_key("authorization") {
                    ResponseTemplate::new(200).set_body_json(json!({"login":"alice"}))
                } else {
                    ResponseTemplate::new(401)
                }
            })
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let error = client(&server, "alice").web_session().await.unwrap_err();
        assert!(matches!(error, GbMcpError::Authentication(_)));
        assert!(!format!("{error:?}").contains("fake-secret"));
    }
}
#[tokio::test]
async fn expired_session_is_rechecked_before_a_write() {
    let server = MockServer::start().await;
    auth(&server, "alice", "alice").await;
    let session = client(&server, "alice").web_session().await.unwrap();
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(1)
        .mount(&server)
        .await;
    assert!(matches!(
        session.edit_issue_title("owner", "repo", 1, "title").await,
        Err(GbMcpError::Authentication(_))
    ));
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/owner/repo/issues/edit_title/1"));
}
#[tokio::test]
async fn credentials_and_form_bodies_are_never_replayed_on_307_or_308() {
    for status in [307, 308] {
        let server = MockServer::start().await;
        auth(&server, "alice", "alice").await;
        Mock::given(method("POST"))
            .and(path("/signin"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", "/collector"))
            .with_priority(1)
            .mount(&server)
            .await;
        assert!(client(&server, "alice").web_session().await.is_err());
        assert!(!server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/collector"));
        let server = MockServer::start().await;
        auth(&server, "alice", "alice").await;
        let session = client(&server, "alice").web_session().await.unwrap();
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/edit_title/1"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", "/collector"))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            session.edit_issue_title("owner", "repo", 1, "title").await,
            Err(GbMcpError::OperationOutcomeUnknown { .. })
        ));
        assert!(!server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/collector"));
    }
}
#[tokio::test]
async fn unrelated_or_mutating_redirect_destinations_are_not_fetched() {
    for destination in [
        "/owner/repo/issues/milestones/3/delete",
        "/owner/another/issues/_data/1",
        "http://elsewhere.test/collector",
        "/signin",
        "/signin/oidc",
    ] {
        let server = MockServer::start().await;
        auth(&server, "alice", "alice").await;
        let session = client(&server, "alice").web_session().await.unwrap();
        Mock::given(method("POST"))
            .and(path("/owner/repo/issues/edit_title/1"))
            .respond_with(ResponseTemplate::new(303).insert_header("Location", destination))
            .mount(&server)
            .await;
        assert!(matches!(
            session.edit_issue_title("owner", "repo", 1, "title").await,
            Err(GbMcpError::OperationOutcomeUnknown { .. })
        ));
        let requests = server.received_requests().await.unwrap();
        assert!(requests.iter().all(|r| matches!(
            r.url.path(),
            "/api/v3/user" | "/signin" | "/" | "/owner/repo/issues/edit_title/1"
        )));
    }
}
#[tokio::test]
async fn ignored_issue_write_stops_before_subsequent_fields_and_reports_unverified() {
    let server = MockServer::start().await;
    auth(&server, "alice", "alice").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":1,"title":"old","body":"old body","state":"open"})),
        )
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
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/_data/1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"title":"old","content":"old body"})),
        )
        .mount(&server)
        .await;
    let error = client(&server, "alice")
        .update_issue(
            "owner",
            "repo",
            1,
            &UpdateIssue {
                title: Some("new".into()),
                body: Some("new body".into()),
                state: Some("closed".into()),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error,GbMcpError::OperationNotVerified{fields,..} if fields==["title"]));
    assert!(!server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/owner/repo/issues/edit/1" || r.url.path().ends_with("/state")));
}
#[tokio::test]
async fn session_actor_change_after_title_write_preserves_partial_unknown_without_more_writes() {
    let server = MockServer::start().await;
    auth(&server, "alice", "alice").await;
    let checks = Arc::new(AtomicUsize::new(0));
    let cookie_checks = checks.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(move |r: &wiremock::Request| {
            let login = if r.headers.contains_key("authorization")
                || cookie_checks.fetch_add(1, Ordering::SeqCst) < 2
            {
                "alice"
            } else {
                "bob"
            };
            ResponseTemplate::new(200).set_body_json(json!({"login":login}))
        })
        .with_priority(1)
        .mount(&server)
        .await;
    let changed = Arc::new(AtomicBool::new(false));
    let read = changed.clone();
    Mock::given(method("GET")).and(path("/api/v3/repos/owner/repo/issues/1")).respond_with(move |_:&wiremock::Request|ResponseTemplate::new(200).set_body_json(json!({"number":1,"title":if read.load(Ordering::SeqCst){"new"}else{"old"},"body":"old body","state":"open"}))).mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .respond_with(move |_: &wiremock::Request| {
            changed.store(true, Ordering::SeqCst);
            ResponseTemplate::new(303).insert_header("Location", "/owner/repo/issues/_data/1")
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/owner/repo/issues/_data/1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"title":"new","content":"old body"})),
        )
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server, "alice")
            .update_issue(
                "owner",
                "repo",
                1,
                &UpdateIssue {
                    title: Some("new".into()),
                    body: Some("new body".into()),
                    state: None
                }
            )
            .await,
        Err(GbMcpError::OperationOutcomeUnknown { .. })
    ));
    assert!(!server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == "/owner/repo/issues/edit/1"));
}
#[tokio::test]
async fn delete_success_is_verified_and_read_failures_are_unknown() {
    for label in [true, false] {
        for status in [200, 403, 404, 500] {
            let server = MockServer::start().await;
            let path_value = if label {
                "/api/v3/repos/owner/repo/labels/name"
            } else {
                "/api/v3/repos/owner/repo/milestones/1"
            };
            Mock::given(method("DELETE"))
                .and(path(path_value))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(path_value))
                .respond_with(if status == 200 {
                    ResponseTemplate::new(200).set_body_json(if label {
                        json!({"name":"name"})
                    } else {
                        json!({"number":1,"title":"title","state":"open"})
                    })
                } else {
                    ResponseTemplate::new(status)
                })
                .expect(1)
                .mount(&server)
                .await;
            let c = GitBucketClient::new(&server.uri(), "fake-token").unwrap();
            let result = if label {
                c.delete_label("owner", "repo", "name").await
            } else {
                c.delete_milestone("owner", "repo", 1).await
            };
            match status {
                404 => assert!(result.is_ok()),
                200 => assert!(matches!(
                    result,
                    Err(GbMcpError::OperationNotVerified { .. })
                )),
                _ => assert!(matches!(
                    result,
                    Err(GbMcpError::OperationOutcomeUnknown { .. })
                )),
            }
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
        }
    }
}

#[tokio::test]
async fn expiry_before_the_first_resource_write_is_an_auth_error_for_all_web_services() {
    for change in [
        "title",
        "body",
        "state",
        "milestone-create",
        "milestone-content",
        "milestone-state",
    ] {
        let server = MockServer::start().await;
        auth(&server, "alice", "alice").await;
        let checks = AtomicUsize::new(0);
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(move |r: &wiremock::Request| {
                if r.headers.contains_key("authorization")
                    || checks.fetch_add(1, Ordering::SeqCst) == 0
                {
                    ResponseTemplate::new(200).set_body_json(json!({"login":"alice"}))
                } else {
                    ResponseTemplate::new(401)
                }
            })
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/issues/1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"number":1,"title":"old","body":"old body","state":"open"}),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/milestones/1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"number":1,"title":"日本語","state":"open"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/milestones"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let c = client(&server, "alice");
        let error = if change == "milestone-create" {
            c.create_milestone(
                "owner",
                "repo",
                &CreateMilestone {
                    title: "日本語".into(),
                    description: None,
                    due_on: None,
                },
            )
            .await
            .unwrap_err()
        } else if change.starts_with("milestone-") {
            c.update_milestone(
                "owner",
                "repo",
                1,
                &UpdateMilestone {
                    title: None,
                    due_on: None,
                    description: (change == "milestone-content").then(|| "new".into()),
                    state: (change == "milestone-state").then(|| "closed".into()),
                },
            )
            .await
            .unwrap_err()
        } else {
            c.update_issue(
                "owner",
                "repo",
                1,
                &UpdateIssue {
                    title: (change == "title").then(|| "new".into()),
                    body: (change == "body").then(|| "new body".into()),
                    state: (change == "state").then(|| "closed".into()),
                },
            )
            .await
            .unwrap_err()
        };
        assert!(
            matches!(error, GbMcpError::Authentication(_)),
            "{change}: {error}"
        );
        let result = gitbucket_mcp_server::tools::response::from_gb_error(error).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["kind"],
            "authentication_error"
        );
        assert!(
            !server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|r| r.url.path().starts_with("/owner/repo/")),
            "{change}"
        );
    }
}

#[tokio::test]
async fn a_different_issue_snapshot_is_rejected_before_signin() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"number":2,"title":"old","state":"open"})),
        )
        .mount(&server)
        .await;
    assert!(client(&server, "alice")
        .update_issue(
            "owner",
            "repo",
            1,
            &UpdateIssue {
                title: Some("new".into()),
                body: None,
                state: None
            }
        )
        .await
        .is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
