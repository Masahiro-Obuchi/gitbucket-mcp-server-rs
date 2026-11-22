mod common;

use common::TestServer;
use gitbucket_mcp_server::{
    api::profile::ApiProfile,
    error::GbMcpError,
    models::{
        operation::{FieldStatus, OutcomeStatus},
        repository::CreateRepository,
    },
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, ResponseTemplate,
};

const ORG: &str = "/api/v3/orgs/team/repos";
const REPO: &str = "/api/v3/repos/team/example";
const RAW: &str = "  日本語 repository  ";

fn repo(owner: &str, name: &str, private: bool) -> Value {
    json!({"name": name, "full_name": format!("{owner}/{name}"), "private": private})
}

fn organization_repo() -> Value {
    let mut value = repo("team", "example", true);
    value["owner"] = json!({"login":"team", "type":"Organization"});
    value["description"] = json!(RAW);
    value["default_branch"] = json!("trunk");
    value
}

async fn reply(http: &TestServer, verb: &str, route: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", "token test-token"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(&http.mock_server)
        .await;
}

fn request() -> CreateRepository {
    CreateRepository {
        name: "example".into(),
        description: Some(RAW.into()),
        is_private: Some(true),
        auto_init: Some(true),
    }
}

#[tokio::test]
async fn organization_creation_sends_one_native_post_and_returns_independently_read_repository() {
    for profile in [ApiProfile::RestOnly, ApiProfile::Standard] {
        let http = TestServer::start().await;
        reply(
            &http,
            "GET",
            "/api/v3/users/team",
            200,
            json!({"login":"team","type":"Organization"}),
        )
        .await;
        Mock::given(method("POST"))
            .and(path(ORG))
            .and(header("authorization", "token test-token"))
            .and(body_json(
                json!({"name":"example","description":RAW,"private":true,"auto_init":true}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(organization_repo()))
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let mut observed = organization_repo();
        observed["html_url"] = json!("https://example.test/team/example");
        reply(&http, "GET", REPO, 200, observed).await;
        reply(
            &http,
            "GET",
            &format!("{REPO}/branches"),
            200,
            json!([{"name":"trunk","commit":{"sha":"a".repeat(40)}}]),
        )
        .await;
        let created = http
            .client()
            .with_api_profile(profile)
            .create_organization_repository("team", &request())
            .await
            .unwrap();
        assert_eq!(
            created.html_url.as_deref(),
            Some("https://example.test/team/example")
        );
        assert_eq!(created.description.as_deref(), Some(RAW));
        let calls = http.mock_server.received_requests().await.unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls
            .iter()
            .all(|r| r.url.query().is_none() && !r.headers.contains_key("cookie")));
    }
}

#[tokio::test]
async fn missing_personal_foreign_or_untyped_organization_stops_before_creation() {
    for (status, account) in [
        (200, json!({"login":"team","type":"User"})),
        (200, json!({"login":"other","type":"Organization"})),
        (200, json!({"login":"team"})),
        (404, json!({})),
    ] {
        let http = TestServer::start().await;
        reply(&http, "GET", "/api/v3/users/team", status, account).await;
        assert!(http
            .client()
            .create_organization_repository("team", &request())
            .await
            .is_err());
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
    let http = TestServer::start().await;
    for (owner, name) in [
        ("../team", "example".to_owned()),
        ("team", "x".repeat(101)),
        ("team", "a/b".to_owned()),
    ] {
        assert!(http
            .client()
            .create_organization_repository(owner, &CreateRepository { name, ..request() })
            .await
            .is_err());
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn denied_or_uncertain_post_keeps_scoped_target_without_replaying() {
    for status in [403, 422, 500, 307] {
        let http = TestServer::start().await;
        reply(
            &http,
            "GET",
            "/api/v3/users/team",
            200,
            json!({"login":"team","type":"Organization"}),
        )
        .await;
        Mock::given(method("POST"))
            .and(path(ORG))
            .respond_with(
                ResponseTemplate::new(status).insert_header("Location", "/api/v3/user/repos"),
            )
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let error = http
            .client()
            .create_organization_repository("team", &request())
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("{error}");
        };
        assert_eq!(
            outcome.status,
            if status < 500 && status != 307 {
                OutcomeStatus::NotApplied
            } else {
                OutcomeStatus::Unknown
            }
        );
        let target = outcome.target.unwrap();
        assert_eq!(target.owner.as_deref(), Some("team"));
        assert_eq!(target.repo, "example");
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn post_acknowledgement_alone_does_not_verify_repository_options_or_initialization() {
    for (route, response) in [
        (REPO, ResponseTemplate::new(404)),
        (
            "/api/v3/repos/team/example/branches",
            ResponseTemplate::new(200).set_body_json(json!([])),
        ),
    ] {
        let http = TestServer::start().await;
        reply(
            &http,
            "GET",
            "/api/v3/users/team",
            200,
            json!({"login":"team","type":"Organization"}),
        )
        .await;
        reply(&http, "POST", ORG, 200, organization_repo()).await;
        if route != REPO {
            reply(&http, "GET", REPO, 200, organization_repo()).await;
        }
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(response)
            .expect(1)
            .mount(&http.mock_server)
            .await;
        let error = http
            .client()
            .create_organization_repository("team", &request())
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("{error}");
        };
        assert_eq!(
            outcome.status,
            if route == REPO {
                OutcomeStatus::Unknown
            } else {
                OutcomeStatus::Partial
            }
        );
    }
}

#[tokio::test]
async fn personal_creation_rejects_invalid_names_and_unaddressable_pat_owners_before_post() {
    let http = TestServer::start().await;
    for name in ["..".into(), "name/extra".into(), "x".repeat(101)] {
        let error = http
            .client()
            .create_repository(&CreateRepository { name, ..request() })
            .await
            .unwrap_err();
        assert!(matches!(error, GbMcpError::InvalidInput(_)));
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());

    for account in [
        json!({"login":"team","type":"Organization"}),
        json!({"login":"owner/name","type":"User"}),
    ] {
        let http = TestServer::start().await;
        reply(&http, "GET", "/api/v3/user", 200, account).await;
        let error = http
            .client()
            .create_repository(&request())
            .await
            .unwrap_err();
        assert!(matches!(error, GbMcpError::ReadConflict(_)));
        assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn personal_creation_fails_when_post_or_readback_returns_a_different_owner() {
    for foreign_acknowledgement in [true, false] {
        let http = TestServer::start().await;
        let owner = json!({"login":"me","type":"User"});
        let mut expected = repo("me", "example", true);
        expected["owner"] = owner.clone();
        let mut foreign = repo("other", "example", true);
        foreign["owner"] = json!({"login":"other","type":"User"});
        reply(&http, "GET", "/api/v3/user", 200, owner).await;
        reply(
            &http,
            "POST",
            "/api/v3/user/repos",
            200,
            if foreign_acknowledgement {
                foreign.clone()
            } else {
                expected
            },
        )
        .await;
        if !foreign_acknowledgement {
            reply(&http, "GET", "/api/v3/repos/me/example", 200, foreign).await;
        }

        let error = http
            .client()
            .create_repository(&request())
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, .. } = error else {
            panic!("missing operation outcome");
        };
        assert_eq!(outcome.status, OutcomeStatus::Unknown);
        let target = outcome.target.unwrap();
        assert_eq!(target.owner.as_deref(), Some("me"));
        assert_eq!(target.repo, "example");
        let expected_requests = if foreign_acknowledgement { 2 } else { 3 };
        assert_eq!(
            http.mock_server.received_requests().await.unwrap().len(),
            expected_requests
        );
    }
}

#[tokio::test]
async fn personal_creation_keeps_pat_owner_and_verified_metadata_when_initialization_fails() {
    let http = TestServer::start().await;
    let login = "builder@example.test";
    let account = json!({"login":login,"type":"User"});
    let mut created = repo(login, "example", true);
    created["owner"] = account.clone();
    created["description"] = json!(RAW);
    created["default_branch"] = json!("trunk");
    reply(&http, "GET", "/api/v3/user", 200, account).await;
    reply(&http, "POST", "/api/v3/user/repos", 200, created.clone()).await;
    let repo_path = format!("/api/v3/repos/{login}/example");
    reply(&http, "GET", &repo_path, 200, created).await;
    reply(
        &http,
        "GET",
        &format!("{repo_path}/branches"),
        200,
        json!([]),
    )
    .await;
    let error = http
        .client()
        .with_api_profile(ApiProfile::RestOnly)
        .create_repository(&request())
        .await
        .unwrap_err();
    let GbMcpError::OperationFailed { outcome, .. } = error else {
        panic!("missing operation outcome");
    };
    assert_eq!(outcome.status, OutcomeStatus::Partial);
    let target = outcome.target.unwrap();
    assert_eq!(target.owner.as_deref(), Some(login));
    assert_eq!(target.repo, "example");
    for field in outcome.fields {
        assert_eq!(
            field.status,
            if field.field == "auto_init" {
                FieldStatus::Mismatched
            } else {
                FieldStatus::Verified
            }
        );
    }
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 4);
}

async fn directories(http: &TestServer, users: Value, orgs: Value) {
    reply(http, "GET", "/api/v3/user", 200, json!({"login":"me"})).await;
    reply(http, "GET", "/api/v3/users", 200, users).await;
    reply(http, "GET", "/api/v3/organizations", 200, orgs).await;
}

#[tokio::test]
async fn visibility_scan_includes_admin_provisioned_email_style_logins() {
    let http = TestServer::start().await;
    let login = "builder@example.test";
    directories(&http, json!([{"login":"me"},{"login":login}]), json!([])).await;
    reply(
        &http,
        "GET",
        "/api/v3/users/builder@example.test/repos",
        200,
        json!([repo(login, "shared", true)]),
    )
    .await;
    reply(&http, "GET", "/api/v3/users/me/repos", 200, json!([])).await;

    let repositories = http.client().list_visible_repositories().await.unwrap();
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].full_name, "builder@example.test/shared");
    assert!(repositories[0].is_private);
}

#[tokio::test]
async fn visibility_scan_rejects_unaddressable_logins_before_querying_owners() {
    let http = TestServer::start().await;
    directories(
        &http,
        json!([{"login":"me"},{"login":"team/member"}]),
        json!([]),
    )
    .await;
    assert!(matches!(
        http.client().list_visible_repositories().await,
        Err(GbMcpError::ReadConflict(_))
    ));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn visibility_scan_rejects_duplicate_or_out_of_scope_repositories_and_directory_overflow() {
    let orgs = json!([{"login":"team"}]);
    let users = json!([{"login":"me"}]);
    for invalid in [
        json!([repo("me", "duplicate", true), repo("me", "duplicate", true)]),
        json!([repo("foreign", "one", true)]),
    ] {
        let http = TestServer::start().await;
        directories(&http, users.clone(), orgs.clone()).await;
        reply(&http, "GET", "/api/v3/users/me/repos", 200, invalid).await;
        assert!(matches!(
            http.client().list_visible_repositories().await,
            Err(GbMcpError::ReadConflict(_))
        ));
    }
    let http = TestServer::start().await;
    let mut many = vec![json!({"login":"me"})];
    many.extend((0..500).map(|n| json!({"login":format!("user-{n}")})));
    directories(&http, json!(many), json!([])).await;
    assert!(matches!(
        http.client().list_visible_repositories().await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_failed_owner_collection_does_not_return_a_complete_subset() {
    let http = TestServer::start().await;
    directories(&http, json!([{"login":"me"}]), json!([{"login":"team"}])).await;
    reply(
        &http,
        "GET",
        "/api/v3/users/me/repos",
        200,
        json!([repo("me", "visible", false)]),
    )
    .await;
    reply(&http, "GET", ORG, 503, json!({})).await;
    assert!(matches!(
        http.client().list_visible_repositories().await,
        Err(GbMcpError::Api { status: 503, .. })
    ));
}

#[tokio::test]
async fn readonly_discovery_pages_a_large_multi_owner_observation_without_refetching() {
    let http = TestServer::start().await;
    directories(
        &http,
        json!([{"login":"me"},{"login":"other"}]),
        json!([{"login":"team"}]),
    )
    .await;
    reply(
        &http,
        "GET",
        "/api/v3/users/me/repos",
        200,
        json!([repo("me", "own", true)]),
    )
    .await;
    reply(
        &http,
        "GET",
        "/api/v3/users/other/repos",
        200,
        json!([repo("other", "shared", true)]),
    )
    .await;
    let group: Vec<_> = (0..126)
        .rev()
        .map(|n| repo("team", &format!("project-{n:03}"), n % 2 == 0))
        .collect();
    reply(&http, "GET", ORG, 200, json!(group)).await;
    let (server_io, client_io) = tokio::io::duplex(65536);
    let server = GitBucketMcpServer::new(http.client()).with_access_mode(AccessMode::ReadOnly);
    let handle = tokio::spawn(async move {
        server
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap()
    });
    let client = ().serve(client_io).await.unwrap();
    let call = |tool: &'static str, args: Value| {
        client.call_tool(
            CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
        )
    };
    let mut args = json!({"limit":50});
    let mut names = Vec::new();
    let mut observation = None;
    loop {
        let result = call("list_visible_repositories", args.clone())
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        let page = result.structured_content.unwrap();
        assert_eq!(page["range"]["source_complete"], true);
        assert_eq!(page["range"]["atomic"], false);
        let id = page["range"]["observation_id"].clone();
        assert_eq!(observation.get_or_insert(id.clone()), &id);
        names.extend(
            page["repositories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["full_name"].as_str().unwrap().to_owned()),
        );
        if page["has_more"] == false {
            break;
        }
        args["cursor"] = page["next_cursor"].clone();
        let wrong_scope = call(
            "list_repositories",
            json!({"owner":"team","limit":50,"cursor":args["cursor"]}),
        )
        .await
        .unwrap();
        assert_eq!(wrong_scope.is_error, Some(true));
    }
    assert_eq!(names.len(), 128);
    assert_eq!(&names[..2], ["me/own", "other/shared"]);
    assert!(names.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(http.mock_server.received_requests().await.unwrap().len(), 6);
    client.cancel().await.unwrap();
    handle.await.unwrap();
}
