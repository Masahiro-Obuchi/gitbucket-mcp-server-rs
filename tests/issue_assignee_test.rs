mod common;
use common::TestServer;
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::{
        issue_assignee::AssigneeNames,
        operation::{FieldStatus, OperationOutcome, OutcomeStatus},
    },
    server::GitBucketMcpServer,
};
use rmcp::{model::CallToolRequestParams, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

const RAW: &str = "  日本語 **Markdown**  \n\n    code\nlast  \n";

fn request(names: &[&str]) -> AssigneeNames {
    AssigneeNames {
        assignees: names.iter().map(|s| (*s).to_owned()).collect(),
    }
}
fn issue(names: &[&str]) -> Value {
    json!({"number":7,"title":"Issue","body":RAW,"state":"closed","labels":[{"name":" exact 日本語 "}],
        "assignees":names.iter().map(|name|json!({"login":name,"type":"User"})).collect::<Vec<_>>(),
        "milestone":{"number":9,"title":"Release","state":"closed"}})
}
fn page(value: &Value) -> String {
    let names: Vec<_> = value["assignees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["login"].as_str().unwrap())
        .collect();
    let people = if names.is_empty() {
        "<span class='muted small'>No one assigned</span>".into()
    } else {
        names.iter().map(|name|format!("<div><a href='/{name}'><img></a><a class='username strong small' href='/{name}'>Display name</a></div>")).collect::<String>()
    };
    let milestone = if value["milestone"].is_null() {
        "<span class='muted small'>No milestone</span>"
    } else {
        "<a href='/owner/repo/issues?milestone=Release'>Release</a>"
    };
    format!(
        "<span id='label-assigned'>{people}</span><span id='label-milestone'>{milestone}</span>"
    )
}

#[derive(Default, Clone)]
struct Options {
    status_at: Option<(usize, u16)>,
    ignore: bool,
    patch: Vec<(&'static str, Value)>,
    missing: Vec<&'static str>,
    get_error_at: Option<(usize, u16)>,
    delay_at: Option<usize>,
    web_before: Option<String>,
    web_after: Option<String>,
    post_body: Option<String>,
    extra_name: Option<String>,
    expire_cookie_at: Option<usize>,
}
struct Fixture {
    http: TestServer,
    posts: Arc<Mutex<Vec<(bool, String)>>>,
}
impl Fixture {
    fn client(&self) -> GitBucketClient {
        self.http
            .standard_client_with_web_auth("alice", "fake-password")
    }
    fn posts(&self) -> Vec<(bool, String)> {
        self.posts.lock().unwrap().clone()
    }
}
async fn fixture(before: Value, options: Options) -> Fixture {
    let http = TestServer::start().await;
    let stored = Arc::new(Mutex::new(before));
    let posts = Arc::new(Mutex::new(Vec::new()));
    let cookie_reads = AtomicUsize::new(0);
    let expire = options.expire_cookie_at;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(move |r: &wiremock::Request| {
            if r.headers.contains_key("authorization")
                || expire.is_none_or(|n| cookie_reads.fetch_add(1, Ordering::SeqCst) < n)
            {
                ResponseTemplate::new(200).set_body_json(json!({"login":"alice","type":"User"}))
            } else {
                ResponseTemplate::new(401)
            }
        })
        .mount(&http.mock_server)
        .await;
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
    for name in ["alice", "bob", "carol", "dave"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/users/{name}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"login":name,"type":"User"})),
            )
            .mount(&http.mock_server)
            .await;
    }
    let (state, attempts, opts) = (stored.clone(), posts.clone(), options.clone());
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/7"))
        .respond_with(move |_: &wiremock::Request| {
            let n = attempts.lock().unwrap().len();
            if let Some((threshold, status)) = opts.get_error_at {
                if n >= threshold {
                    return ResponseTemplate::new(status);
                }
            }
            let mut value = state.lock().unwrap().clone();
            if n > 0 {
                for (key, v) in &opts.patch {
                    value[*key] = v.clone();
                }
                for key in &opts.missing {
                    value.as_object_mut().unwrap().remove(*key);
                }
            }
            let mut reply = ResponseTemplate::new(200).set_body_json(value);
            if opts.delay_at.is_some_and(|threshold| n >= threshold) {
                reply = reply.set_delay(Duration::from_secs(2));
            }
            reply
        })
        .mount(&http.mock_server)
        .await;
    for kind in ["issues", "pull"] {
        let (state, attempts, opts) = (stored.clone(), posts.clone(), options.clone());
        Mock::given(method("GET"))
            .and(path(format!("/owner/repo/{kind}/7")))
            .respond_with(move |_: &wiremock::Request| {
                let n = attempts.lock().unwrap().len();
                let html = if n == 0 {
                    opts.web_before.clone()
                } else {
                    opts.web_after.clone()
                };
                ResponseTemplate::new(200)
                    .set_body_string(html.unwrap_or_else(|| page(&state.lock().unwrap())))
            })
            .mount(&http.mock_server)
            .await;
    }
    for add in [true, false] {
        let (state, attempts, opts) = (stored.clone(), posts.clone(), options.clone());
        Mock::given(method("POST"))
            .and(path(format!(
                "/owner/repo/issues/7/assignee/{}",
                if add { "new" } else { "delete" }
            )))
            .respond_with(move |r: &wiremock::Request| {
                let fields: Vec<_> = url::form_urlencoded::parse(&r.body).collect();
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "assigneeUserName");
                assert!(!r.headers.contains_key("authorization"));
                assert!(r.headers.contains_key("cookie"));
                assert_eq!(r.headers["x-requested-with"], "XMLHttpRequest");
                let name = fields[0].1.to_string();
                let n = {
                    let mut sent = attempts.lock().unwrap();
                    sent.push((add, name.clone()));
                    sent.len()
                };
                if let Some((at, status)) = opts.status_at {
                    if n == at {
                        return ResponseTemplate::new(status)
                            .insert_header("Location", "/must-not-follow");
                    }
                }
                if !opts.ignore {
                    let mut value = state.lock().unwrap();
                    let mut names: BTreeSet<_> = value["assignees"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|u| u["login"].as_str().unwrap().to_owned())
                        .collect();
                    if add {
                        assert!(names.insert(name));
                    } else {
                        assert!(names.remove(&name));
                    }
                    if let Some(extra) = &opts.extra_name {
                        names.insert(extra.clone());
                    }
                    value["assignees"] = json!(names
                        .iter()
                        .map(|name| json!({"login":name,"type":"User"}))
                        .collect::<Vec<_>>());
                }
                ResponseTemplate::new(200)
                    .set_body_string(opts.post_body.clone().unwrap_or_default())
            })
            .mount(&http.mock_server)
            .await;
    }
    Fixture { http, posts }
}
fn outcome(error: GbMcpError) -> OperationOutcome {
    match error {
        GbMcpError::OperationFailed { outcome, .. } => *outcome,
        error => panic!("Expected operation evidence: {error:?}"),
    }
}
fn status(outcome: &OperationOutcome, field: &str) -> FieldStatus {
    outcome
        .fields
        .iter()
        .find(|f| f.field == field)
        .unwrap()
        .status
}

#[tokio::test]
async fn add_sends_only_missing_accounts_and_preserves_raw_content() {
    let f = fixture(issue(&["alice"]), Options::default()).await;
    let result = f
        .client()
        .add_issue_assignees("owner", "repo", 7, &request(&["carol", "alice", "bob"]))
        .await
        .unwrap();
    assert_eq!(result.body.as_deref(), Some(RAW));
    assert_eq!(result.state, "closed");
    assert_eq!(result.milestone.unwrap().number, 9);
    assert_eq!(
        f.posts(),
        vec![(true, "bob".into()), (true, "carol".into())]
    );
    assert_eq!(result.assignees.unwrap().len(), 3);
}

#[tokio::test]
async fn replace_adds_before_removing_and_verifies_every_intermediate_set() {
    let f = fixture(issue(&["alice", "bob"]), Options::default()).await;
    let result = f
        .client()
        .replace_issue_assignees("owner", "repo", 7, &request(&["carol"]))
        .await
        .unwrap();
    assert_eq!(
        f.posts(),
        vec![
            (true, "carol".into()),
            (false, "alice".into()),
            (false, "bob".into())
        ]
    );
    assert_eq!(result.assignees.unwrap()[0].login, "carol");
}

#[tokio::test]
async fn remove_and_clear_preserve_an_explicitly_observed_absent_milestone() {
    let mut before = issue(&["alice", "bob"]);
    before.as_object_mut().unwrap().remove("milestone");
    before["pull_request"] = json!({"url":"http://example.test/pulls/7"});
    let f = fixture(before, Options::default()).await;
    let result = f
        .client()
        .remove_issue_assignee("owner", "repo", 7, "bob")
        .await
        .unwrap();
    assert_eq!(result.assignees.unwrap().len(), 1);
    assert!(result.milestone.is_none());
    let result = f
        .client()
        .replace_issue_assignees("owner", "repo", 7, &request(&[]))
        .await
        .unwrap();
    assert_eq!(result.assignees.unwrap().len(), 0);
    assert_eq!(
        f.posts(),
        vec![(false, "bob".into()), (false, "alice".into())]
    );
}

#[tokio::test]
async fn matching_and_absent_noops_still_require_complete_authenticated_observations() {
    let f = fixture(issue(&["alice"]), Options::default()).await;
    f.client()
        .add_issue_assignees("owner", "repo", 7, &request(&["alice"]))
        .await
        .unwrap();
    f.client()
        .remove_issue_assignee("owner", "repo", 7, "bob")
        .await
        .unwrap();
    f.client()
        .replace_issue_assignees("owner", "repo", 7, &request(&["alice"]))
        .await
        .unwrap();
    assert!(f.posts().is_empty());
    assert!(matches!(
        f.http
            .standard_client()
            .add_issue_assignees("owner", "repo", 7, &request(&["alice"]))
            .await,
        Err(GbMcpError::Authentication(_))
    ));
}

#[tokio::test]
async fn rest_only_and_invalid_identifiers_send_no_http() {
    let http = TestServer::start().await;
    let c = http.standard_client_with_web_auth("alice", "fake-password");
    for names in [
        &[][..],
        &["bob", "bob"][..],
        &[".."][..],
        &[" bob"][..],
        &["a/b"][..],
    ] {
        assert!(matches!(
            c.add_issue_assignees("owner", "repo", 7, &request(names))
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    for n in [0, i32::MAX as u64 + 1, u64::MAX] {
        assert!(matches!(
            c.remove_issue_assignee("owner", "repo", n, "bob").await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    let c = c.with_api_profile(ApiProfile::RestOnly);
    assert!(matches!(
        c.add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
            .await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert!(matches!(
        c.replace_issue_assignees("owner", "repo", 7, &request(&[]))
            .await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert!(matches!(
        c.remove_issue_assignee("owner", "repo", 7, "bob").await,
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
async fn incomplete_parent_wrong_identity_and_account_types_fail_before_signin() {
    for case in [
        "number",
        "body",
        "labels",
        "assignees",
        "duplicate",
        "organization",
        "unknown_type",
        "missing_account",
        "account_id",
    ] {
        let mut before = issue(&["alice"]);
        if case == "number" {
            before["number"] = json!(8);
        }
        if ["body", "labels", "assignees"].contains(&case) {
            before.as_object_mut().unwrap().remove(case);
        }
        if case == "duplicate" {
            before["assignees"] =
                json!([{"login":"alice","type":"User"},{"login":"alice","type":"User"}]);
        }
        let f = fixture(before, Options::default()).await;
        if [
            "organization",
            "unknown_type",
            "missing_account",
            "account_id",
        ]
        .contains(&case)
        {
            let user = json!({"login":if case=="account_id" {"carol"}else{"bob"},"type":if case=="organization" {json!("Organization")}else if case=="unknown_type" {Value::Null}else{json!("User")}});
            Mock::given(method("GET"))
                .and(path("/api/v3/users/bob"))
                .respond_with(if case == "missing_account" {
                    ResponseTemplate::new(404)
                } else {
                    ResponseTemplate::new(200).set_body_json(user)
                })
                .with_priority(1)
                .mount(&f.http.mock_server)
                .await;
        }
        assert!(
            f.client()
                .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
                .await
                .is_err(),
            "{case}"
        );
        assert!(f.posts().is_empty());
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
async fn hidden_removed_or_ambiguous_sidebar_assignments_are_not_empty_noops() {
    let normal = page(&issue(&[]));
    for html in [
        normal.repeat(2),
        normal.replace(
            "<span class='muted small'>No one assigned</span>",
            "<div><img>disabled account</div>",
        ),
        normal.replace("<span class='muted small'>No one assigned</span>", ""),
        page(&issue(&["bob"])),
        normal.replace("id='label-assigned'", "id='other'"),
    ] {
        let f = fixture(
            issue(&[]),
            Options {
                web_before: Some(html),
                ..Default::default()
            },
        )
        .await;
        assert!(f
            .client()
            .replace_issue_assignees("owner", "repo", 7, &request(&[]))
            .await
            .is_err());
        assert!(f.posts().is_empty());
    }
}

#[tokio::test]
async fn ignored_write_is_not_verified_and_no_later_account_is_sent() {
    let f = fixture(
        issue(&["alice"]),
        Options {
            ignore: true,
            ..Default::default()
        },
    )
    .await;
    let o = outcome(
        f.client()
            .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
            .await
            .unwrap_err(),
    );
    assert_eq!(o.status, OutcomeStatus::NotVerified);
    assert_eq!(status(&o, "assignee.bob"), FieldStatus::Mismatched);
    assert_eq!(f.posts(), vec![(true, "bob".into())]);
}

#[tokio::test]
async fn rejected_second_step_retains_verified_prefix_and_stops_removals() {
    let f = fixture(
        issue(&["alice"]),
        Options {
            status_at: Some((2, 403)),
            ..Default::default()
        },
    )
    .await;
    let o = outcome(
        f.client()
            .replace_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
            .await
            .unwrap_err(),
    );
    assert_eq!(o.status, OutcomeStatus::Partial);
    assert_eq!(status(&o, "assignee.bob"), FieldStatus::Verified);
    assert_eq!(status(&o, "assignee.carol"), FieldStatus::Rejected);
    assert_eq!(status(&o, "assignee.alice"), FieldStatus::NotAttempted);
    assert_eq!(o.target.unwrap().number, Some(7));
    assert_eq!(
        f.posts(),
        vec![(true, "bob".into()), (true, "carol".into())]
    );
}

#[tokio::test]
async fn post_write_get_errors_retain_target_prefix_and_unknown_pending_account() {
    for (at, expected) in [(1, OutcomeStatus::Unknown), (2, OutcomeStatus::Partial)] {
        for code in [401, 403, 404, 500] {
            let f = fixture(
                issue(&["alice"]),
                Options {
                    get_error_at: Some((at, code)),
                    ..Default::default()
                },
            )
            .await;
            let o = outcome(
                f.client()
                    .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
                    .await
                    .unwrap_err(),
            );
            assert_eq!(o.status, expected);
            if at == 2 {
                assert_eq!(status(&o, "assignee.bob"), FieldStatus::Verified);
            }
            assert_eq!(o.target.unwrap().number, Some(7));
            assert_eq!(f.posts().len(), at);
        }
    }
}

#[tokio::test]
async fn unexpected_write_replies_are_unknown_and_never_redirect_or_replay() {
    for code in [401, 403, 404, 408, 422, 500, 302, 307, 308] {
        let f = fixture(
            issue(&[]),
            Options {
                status_at: Some((1, code)),
                ..Default::default()
            },
        )
        .await;
        let o = outcome(
            f.client()
                .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
                .await
                .unwrap_err(),
        );
        assert_eq!(
            o.status,
            if [401, 403].contains(&code) {
                OutcomeStatus::NotApplied
            } else {
                OutcomeStatus::Unknown
            }
        );
        assert_eq!(f.posts().len(), 1);
        assert!(!f
            .http
            .mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.url.path() == "/must-not-follow"));
    }
    let f = fixture(
        issue(&[]),
        Options {
            post_body: Some(
                "<form action='/signin'><input type='password' value='fake-secret'></form>".into(),
            ),
            ..Default::default()
        },
    )
    .await;
    let error = f
        .client()
        .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
        .await
        .unwrap_err();
    assert!(!format!("{error:?}").contains("fake-secret"));
    assert_eq!(outcome(error).status, OutcomeStatus::Unknown);
}

#[tokio::test]
async fn response_identity_mismatch_preserves_intended_and_observed_numbers() {
    let f = fixture(
        issue(&[]),
        Options {
            patch: vec![("number", json!(8))],
            ..Default::default()
        },
    )
    .await;
    let o = outcome(
        f.client()
            .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
            .await
            .unwrap_err(),
    );
    assert_eq!(o.status, OutcomeStatus::Unknown);
    let t = o.target.unwrap();
    assert_eq!(t.number, Some(7));
    assert_eq!(t.observed_number, Some(8));
}

#[tokio::test]
async fn preserved_field_mismatch_keeps_the_verified_account_change() {
    for (field, value) in [
        ("title", json!("changed")),
        ("body", json!("changed")),
        ("state", json!("open")),
        ("labels", json!([])),
        (
            "milestone",
            json!({"number":8,"title":"Other","state":"open"}),
        ),
    ] {
        let f = fixture(
            issue(&[]),
            Options {
                patch: vec![(field, value)],
                ..Default::default()
            },
        )
        .await;
        let o = outcome(
            f.client()
                .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
                .await
                .unwrap_err(),
        );
        assert_eq!(o.status, OutcomeStatus::Partial, "{field}");
        assert_eq!(status(&o, "assignee.bob"), FieldStatus::Verified);
        assert_eq!(status(&o, field), FieldStatus::Mismatched);
        assert_eq!(f.posts().len(), 1);
    }
}

#[tokio::test]
async fn absent_or_conflicting_post_write_observations_do_not_become_empty_sets() {
    for field in ["assignees", "body", "labels", "milestone"] {
        let f = fixture(
            issue(&[]),
            Options {
                missing: vec![field],
                ..Default::default()
            },
        )
        .await;
        let o = outcome(
            f.client()
                .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
                .await
                .unwrap_err(),
        );
        assert_eq!(
            o.status,
            if field == "assignees" {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::Partial
            }
        );
        assert_eq!(f.posts().len(), 1);
    }
    let f = fixture(
        issue(&[]),
        Options {
            web_after: Some(page(&issue(&[]))),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        outcome(
            f.client()
                .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
                .await
                .unwrap_err()
        )
        .status,
        OutcomeStatus::Unknown
    );
}

#[tokio::test]
async fn unexpected_concurrent_set_stops_the_remaining_plan() {
    let f = fixture(
        issue(&[]),
        Options {
            extra_name: Some("carol".into()),
            ..Default::default()
        },
    )
    .await;
    let o = outcome(
        f.client()
            .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
            .await
            .unwrap_err(),
    );
    assert_eq!(o.status, OutcomeStatus::Partial);
    assert_eq!(status(&o, "assignee.bob"), FieldStatus::Verified);
    assert_eq!(f.posts().len(), 1);
}

#[tokio::test]
async fn identity_mismatch_or_cookie_expiry_prevents_unchecked_writes() {
    let f = fixture(issue(&[]), Options::default()).await;
    assert!(matches!(
        f.http
            .standard_client_with_web_auth("different", "fake-password")
            .add_issue_assignees("owner", "repo", 7, &request(&["bob"]))
            .await,
        Err(GbMcpError::Authentication(_))
    ));
    assert!(f.posts().is_empty());
    // Cookie checks: sign-in, baseline before/after, first POST, first page
    // before/after, then the second POST. Expire only at that next attempt.
    let f = fixture(
        issue(&[]),
        Options {
            expire_cookie_at: Some(6),
            ..Default::default()
        },
    )
    .await;
    let o = outcome(
        f.client()
            .add_issue_assignees("owner", "repo", 7, &request(&["bob", "carol"]))
            .await
            .unwrap_err(),
    );
    assert_eq!(o.status, OutcomeStatus::Partial);
    assert_eq!(status(&o, "assignee.bob"), FieldStatus::Verified);
    assert_eq!(f.posts().len(), 1);
}

async fn mcp(
    client: GitBucketClient,
    timeout: Duration,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let server = GitBucketMcpServer::with_api_and_timeout(Arc::new(client), timeout).unwrap();
    let (server_io, client_io) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.unwrap()
}

#[tokio::test]
async fn mcp_normalizes_identifiers_and_keeps_raw_content_and_partial_deadline_evidence() {
    let f = fixture(issue(&[]), Options::default()).await;
    let client = mcp(f.client(), Duration::from_secs(10)).await;
    let params = json!({"owner":" owner ","repo":" repo ","issue_number":7,"assignees":[" bob "]});
    let result = client
        .call_tool(
            CallToolRequestParams::new("add_issue_assignees")
                .with_arguments(params.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["body"], RAW);
    assert_eq!(f.posts(), vec![(true, "bob".into())]);
    client.cancel().await.unwrap();

    let f = fixture(
        issue(&[]),
        Options {
            delay_at: Some(2),
            ..Default::default()
        },
    )
    .await;
    let client = mcp(f.client(), Duration::from_millis(600)).await;
    let params =
        json!({"owner":"owner","repo":"repo","issue_number":7,"assignees":["bob","carol"]});
    let result = client
        .call_tool(
            CallToolRequestParams::new("add_issue_assignees")
                .with_arguments(params.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let value = result.structured_content.unwrap();
    assert_eq!(value["operation"]["status"], "partial");
    assert_eq!(value["operation"]["target"]["number"], 7);
    assert_eq!(value["cause"]["kind"], "timeout_error");
    assert!(value["operation"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["field"] == "assignee.bob" && f["status"] == "verified"));
    assert_eq!(f.posts().len(), 2);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_cancellation_keeps_verified_prefix_and_stops_pending_removals() {
    let f = fixture(
        issue(&["alice"]),
        Options {
            delay_at: Some(2),
            ..Default::default()
        },
    )
    .await;
    let server = GitBucketMcpServer::new(f.client());
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
        rmcp::service::RequestContext::new(rmcp::model::RequestId::Number(123), rx.await.unwrap());
    let cancel = context.ct.clone();
    let params = CallToolRequestParams::new("replace_issue_assignees").with_arguments(
        json!({"owner":"owner","repo":"repo","issue_number":7,"assignees":["bob","carol"]})
            .as_object()
            .unwrap()
            .clone(),
    );
    let task = tokio::spawn(async move { server.call_tool(params, context).await.unwrap() });
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.posts().len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    let data = result.structured_content.unwrap();
    assert_eq!(data["operation"]["status"], "partial");
    assert_eq!(data["operation"]["target"]["number"], 7);
    assert_eq!(data["cause"]["kind"], "cancelled_error");
    assert!(data["operation"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field["field"] == "assignee.bob" && field["status"] == "verified"));
    assert_eq!(
        f.posts(),
        vec![(true, "bob".into()), (true, "carol".into())]
    );
    peer.cancel().await.unwrap();
}
