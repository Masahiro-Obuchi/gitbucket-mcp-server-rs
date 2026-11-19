mod common;

use common::{web_identity, TestServer};
use gitbucket_mcp_server::{
    api::{client::GitBucketClient, profile::ApiProfile},
    error::GbMcpError,
    models::{
        operation::{FieldStatus, OutcomeStatus},
        pull_request::CreateForkPullRequest,
    },
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use wiremock::{
    matchers::{method, path, query_param},
    Mock, ResponseTemplate,
};

const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BASE_TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";
const RAW: &str = "日本語 **Markdown**  \n\n    code\tvalue\nend";
const POST: &str = "/gitbucket/upstream/repo/pulls/new";
const CREATED: &str = "/gitbucket/upstream/repo/pull/7";
const PR_READ: &str = "/gitbucket/api/v3/repos/upstream/repo/pulls/7";

fn change() -> CreateForkPullRequest {
    CreateForkPullRequest {
        head_owner: "alice".into(),
        head_repo: "renamed".into(),
        head: "feature/topic".into(),
        base: "release/main".into(),
        title: "PR 日本語".into(),
        body: Some(RAW.into()),
        draft: Some(true),
    }
}

fn repository(owner: &str, repo: &str) -> Value {
    json!({"owner":{"login":owner},"name":repo,"full_name":format!("{owner}/{repo}"),"private":true})
}

fn created() -> Value {
    json!({"number":7,"title":change().title,"body":RAW,"state":"open","merged":false,"draft":true,
        "user":{"login":"alice"},
        "head":{"ref":"feature/topic","sha":HEAD,"repo":repository("alice","renamed")},
        "base":{"ref":"release/main","sha":BASE,"repo":repository("upstream","repo")}})
}

fn page() -> String {
    let fields = [
        ("targetUserName", "upstream"),
        ("targetBranch", "release/main"),
        ("requestUserName", "alice"),
        ("requestRepositoryName", "renamed"),
        ("requestBranch", "feature/topic"),
        ("commitIdFrom", BASE),
        ("commitIdTo", HEAD),
        ("isDraft", "false"),
    ];
    let mut page = format!(r#"<div id="pull-request-form"><form method="POST" action="{POST}">"#);
    for (name, value) in fields {
        page.push_str(&format!(
            r#"<input type="hidden" name="{name}" value="{value}">"#
        ));
    }
    page.push_str("</form></div>");
    page
}

fn client(http: &TestServer, profile: ApiProfile) -> GitBucketClient {
    GitBucketClient::new_with_web_auth(
        &format!("{}/gitbucket", http.mock_server.uri()),
        "test-token",
        false,
        Some("alice"),
        Some("test-password"),
    )
    .unwrap()
    .with_api_profile(profile)
}

async fn fixture(
    html: String,
    moving: bool,
    response: ResponseTemplate,
    readback: ResponseTemplate,
) -> TestServer {
    let http = TestServer::start().await;
    let server = &http.mock_server;
    web_identity(server, "/gitbucket", "alice").await;
    Mock::given(method("POST"))
        .and(path("/gitbucket/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", "/gitbucket/")
                .insert_header("Set-Cookie", "JSESSIONID=fake; Path=/gitbucket"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/gitbucket/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Home</html>"))
        .mount(server)
        .await;
    for (owner, repo, branch, tip) in [
        ("upstream", "repo", "release/main", BASE_TIP),
        ("alice", "renamed", "feature/topic", HEAD),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/gitbucket/api/v3/repos/{owner}/{repo}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(repository(owner, repo)))
            .mount(server)
            .await;
        let reads = AtomicUsize::new(0);
        Mock::given(method("GET"))
            .and(path(format!(
                "/gitbucket/api/v3/repos/{owner}/{repo}/branches/{branch}"
            )))
            .respond_with(move |_: &wiremock::Request| {
                let moved = reads.fetch_add(1, Ordering::SeqCst) > 0 && moving && owner == "alice";
                ResponseTemplate::new(200).set_body_json(
                    json!({"name":branch,"commit":{"sha": if moved { BASE_TIP } else { tip }}}),
                )
            })
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!(
            "/gitbucket/api/v3/repos/upstream/repo/commits/{BASE}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":BASE})))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/gitbucket/alice/renamed/compare/upstream:release/main...feature/topic",
        ))
        .and(query_param("check", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(POST))
        .respond_with(response)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(PR_READ))
        .respond_with(readback)
        .mount(server)
        .await;
    http
}

async fn post_count(http: &TestServer) -> usize {
    http.mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() != "/gitbucket/signin")
        .count()
}

#[tokio::test]
async fn creates_from_explicit_fork_with_one_cookie_post_and_reads_with_pat() {
    let http = fixture(
        page(),
        false,
        ResponseTemplate::new(302).insert_header("Location", format!("{CREATED};jsessionid=fake")),
        ResponseTemplate::new(200).set_body_json(created()),
    )
    .await;
    let actual = client(&http, ApiProfile::Standard)
        .create_fork_pull_request("upstream", "repo", &change())
        .await
        .unwrap();
    assert_eq!(actual.number, 7);
    assert_eq!(actual.draft, Some(true));
    assert_eq!(actual.body.as_deref(), Some(RAW));
    assert_eq!(post_count(&http).await, 1);
    let requests = http.mock_server.received_requests().await.unwrap();
    let post = requests.iter().position(|r| r.url.path() == POST).unwrap();
    let read = requests
        .iter()
        .position(|r| r.url.path() == PR_READ)
        .unwrap();
    assert!(post < read);
    assert!(!requests[post].headers.contains_key("authorization"));
    assert_eq!(requests[post].headers["cookie"], "JSESSIONID=fake");
    assert_eq!(requests[read].headers["authorization"], "token test-token");
    assert!(!requests.iter().any(|r| r.url.path().starts_with(CREATED)));
    let fields: BTreeMap<_, _> = url::form_urlencoded::parse(&requests[post].body)
        .into_owned()
        .collect();
    assert_eq!(
        fields,
        [
            ("title", change().title.as_str()),
            ("content", RAW),
            ("targetUserName", "upstream"),
            ("targetBranch", "release/main"),
            ("requestUserName", "alice"),
            ("requestRepositoryName", "renamed"),
            ("requestBranch", "feature/topic"),
            ("commitIdFrom", BASE),
            ("commitIdTo", HEAD),
            ("isDraft", "true"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    );
}

#[tokio::test]
async fn rejects_ambiguous_foreign_or_stale_compare_forms_before_post() {
    for html in [
        page().replace(POST, "https://outside.example/pulls/new"),
        page().replace(
            r#"name="requestRepositoryName" value="renamed""#,
            r#"name="requestRepositoryName" value="repo""#,
        ),
        page().replace(
            "</form>",
            r#"<input type="hidden" name="requestBranch" value="feature/topic"></form>"#,
        ),
        page().replace("pull-request-form", "foreign-form"),
        page().replace(BASE, "invalid-sha"),
        page().replace(HEAD, BASE_TIP),
    ] {
        let http = fixture(
            html,
            false,
            ResponseTemplate::new(500),
            ResponseTemplate::new(500),
        )
        .await;
        let error = client(&http, ApiProfile::Standard)
            .create_fork_pull_request("upstream", "repo", &change())
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                GbMcpError::Unsupported(_) | GbMcpError::ReadConflict(_)
            ),
            "{error:?}"
        );
        assert_eq!(post_count(&http).await, 0);
    }
    let http = fixture(
        page(),
        true,
        ResponseTemplate::new(500),
        ResponseTemplate::new(500),
    )
    .await;
    assert!(matches!(
        client(&http, ApiProfile::Standard)
            .create_fork_pull_request("upstream", "repo", &change())
            .await,
        Err(GbMcpError::ReadConflict(_))
    ));
    assert_eq!(post_count(&http).await, 0);
}

#[tokio::test]
async fn rejects_rest_only_and_unrepresentable_input_before_http() {
    let http = TestServer::start().await;
    assert!(matches!(
        client(&http, ApiProfile::RestOnly)
            .create_fork_pull_request("upstream", "repo", &change())
            .await,
        Err(GbMcpError::Unsupported(_))
    ));
    let client = client(&http, ApiProfile::Standard);
    for input in [
        CreateForkPullRequest {
            body: Some(format!(" {RAW}")),
            ..change()
        },
        CreateForkPullRequest {
            body: Some("line\r\nend".into()),
            ..change()
        },
        CreateForkPullRequest {
            title: "\tTitle".into(),
            ..change()
        },
        CreateForkPullRequest {
            head: "wrong:branch".into(),
            ..change()
        },
        CreateForkPullRequest {
            base: "main/../target".into(),
            ..change()
        },
    ] {
        assert!(matches!(
            client
                .create_fork_pull_request("upstream", "repo", &input)
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn malformed_redirect_or_failed_readback_never_replays_and_keeps_known_number() {
    for (response, expected) in [
        (
            ResponseTemplate::new(307).insert_header("Location", CREATED),
            None,
        ),
        (
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{CREATED}?unexpected=yes")),
            None,
        ),
        (
            ResponseTemplate::new(302).insert_header(
                "Location",
                "https://outside.example/gitbucket/upstream/repo/pull/7",
            ),
            None,
        ),
        (
            ResponseTemplate::new(302).insert_header("Location", "/gitbucket/alice/renamed/pull/7"),
            None,
        ),
        (ResponseTemplate::new(404), None),
        (
            ResponseTemplate::new(303).insert_header("Location", CREATED),
            Some(7),
        ),
    ] {
        let http = fixture(page(), false, response, ResponseTemplate::new(500)).await;
        let error = client(&http, ApiProfile::Standard)
            .create_fork_pull_request("upstream", "repo", &change())
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("{error:?}")
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        assert_eq!(outcome.target.unwrap().number, expected);
        assert_eq!(post_count(&http).await, 1);
    }
}

#[tokio::test]
async fn rejects_created_pr_with_different_head_or_body_and_preserves_observations() {
    let mut observed = created();
    observed["body"] = json!("different body");
    observed["head"]["repo"] = repository("alice", "repo");
    observed["draft"] = Value::Null;
    let http = fixture(
        page(),
        false,
        ResponseTemplate::new(302).insert_header("Location", CREATED),
        ResponseTemplate::new(200).set_body_json(observed),
    )
    .await;
    let error = client(&http, ApiProfile::Standard)
        .create_fork_pull_request("upstream", "repo", &change())
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("{error:?}")
    };
    assert_eq!(outcome.status, OutcomeStatus::Partial);
    assert_eq!(outcome.target.unwrap().number, Some(7));
    for (field, expected) in [
        ("body", FieldStatus::Mismatched),
        ("head", FieldStatus::Mismatched),
        ("base", FieldStatus::Verified),
        ("draft", FieldStatus::Unknown),
    ] {
        assert_eq!(
            outcome
                .fields
                .iter()
                .find(|f| f.field == field)
                .unwrap()
                .status,
            expected
        );
    }
    assert_eq!(post_count(&http).await, 1);
}

#[tokio::test]
async fn preserves_write_status_and_distinguishes_authentication_rejection_from_uncertainty() {
    for (http_status, expected) in [
        (403, OutcomeStatus::NotApplied),
        (404, OutcomeStatus::Unknown),
    ] {
        let http = fixture(
            page(),
            false,
            ResponseTemplate::new(http_status),
            ResponseTemplate::new(500),
        )
        .await;
        let error = client(&http, ApiProfile::Standard)
            .create_fork_pull_request("upstream", "repo", &change())
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, cause } = error else {
            panic!("{error:?}")
        };
        assert_eq!(outcome.status, expected);
        assert!(matches!(*cause, GbMcpError::Api { status, .. } if status == http_status));
        assert_eq!(post_count(&http).await, 1);
    }
}
