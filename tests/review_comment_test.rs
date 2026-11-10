use gitbucket_mcp_server::{
    api::{client::GitBucketClient, http::HttpSettings, profile::ApiProfile},
    error::GbMcpError,
    models::review_comment::ReviewLookupStatus,
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn pr(head: &str) -> Value {
    json!({"number":1,"title":"Review fixture","state":"open","head":{"ref":"feature","sha":head},"base":{"ref":"main","sha":"a".repeat(40)}})
}

fn page(prefix: &str, cards: &str, badge: usize) -> String {
    format!("<span id='show-title'>Review fixture</span><ul id='pullreq-tab'><li class='active'><a href='{prefix}/probe-owner/probe-repo/pull/1'>Conversation<span class='badge'>{badge}</span></a></li><li><a href='{prefix}/probe-owner/probe-repo/pull/1/commits'>Commits</a></li><li><a href='{prefix}/probe-owner/probe-repo/pull/1/files'>Files</a></li></ul><div id='comment-list'><div class='issue-comment-box'><div id='issueContent'>body</div></div>{cards}</div>")
}

fn captured(prefix: &str) -> String {
    include_str!("fixtures/inline_comments/4.48.0/conversation_before.html")
        .replace("/commit/FIRST", &format!("/commit/{}", "a".repeat(40)))
        .replace("\"/probe-owner", &format!("\"{prefix}/probe-owner"))
        .replace("\"/probe-reader", &format!("\"{prefix}/probe-reader"))
}

async fn fixture(
    prefix: &str,
    response: ResponseTemplate,
    changed: bool,
    settings: HttpSettings,
) -> (MockServer, GitBucketClient) {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path(format!(
            "{prefix}/api/v3/repos/probe-owner/probe-repo/pulls/1"
        )))
        .respond_with(move |_: &wiremock::Request| {
            let changed = changed && counter.fetch_add(1, Ordering::SeqCst) > 0;
            ResponseTemplate::new(200).set_body_json(pr(&if changed {
                "c".repeat(40)
            } else {
                "b".repeat(40)
            }))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/api/v3/user")))
        .respond_with(|r: &wiremock::Request| {
            if r.headers.contains_key("authorization") || r.headers.contains_key("cookie") {
                ResponseTemplate::new(200).set_body_json(json!({"login":"probe-owner"}))
            } else {
                ResponseTemplate::new(401)
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{prefix}/signin")))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", format!("{prefix}/"))
                .insert_header("Set-Cookie", "JSESSIONID=fake-cookie; Path=/"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html>Authenticated landing</html>"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{prefix}/probe-owner/probe-repo/pull/1")))
        .respond_with(response)
        .mount(&server)
        .await;
    let client = GitBucketClient::new_with_web_auth_and_http_settings(
        &format!("{}{prefix}/api/v3", server.uri()),
        "fake-pat",
        false,
        Some("probe-owner"),
        Some("fake-password"),
        settings,
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard);
    (server, client)
}

#[tokio::test]
async fn list_and_lookup_observe_pr_scope_without_raw_or_global_id_requests() {
    for prefix in ["", "/team/api/v3"] {
        let (http, client) = fixture(
            prefix,
            ResponseTemplate::new(200).set_body_string(page(prefix, &captured(prefix), 10)),
            false,
            HttpSettings::default(),
        )
        .await;
        let result = client
            .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
            .await
            .unwrap();
        assert_eq!(result.comments.len(), 11);
        assert!(
            !result.coverage.complete
                && !result.coverage.body_raw_complete
                && !result.coverage.historical_complete
        );
        assert_eq!(result.coverage.observed_head, Some("b".repeat(40)));
        let found = client
            .get_pull_request_review_comment("probe-owner", "probe-repo", 1, "commit:3")
            .await
            .unwrap();
        assert_eq!(found.status, ReviewLookupStatus::Found);
        assert!(found.comment.unwrap().body_raw.is_none());
        let missing = client
            .get_pull_request_review_comment("probe-owner", "probe-repo", 1, "commit:999")
            .await
            .unwrap();
        assert_eq!(missing.status, ReviewLookupStatus::NotObserved);
        assert!(missing.comment.is_none());
        let requests = http.received_requests().await.unwrap();
        for request in requests {
            assert!(!request.url.path().contains("commit_comments"));
            if request.method != "GET" {
                assert_eq!(request.url.path(), format!("{prefix}/signin"));
            }
            if !request.url.path().contains("/api/v3/") {
                assert!(!request.headers.contains_key("authorization"));
            }
            if request.url.path().ends_with("/pull/1") {
                assert!(request.headers.contains_key("cookie"));
            }
        }
    }
}

#[tokio::test]
async fn known_empty_page_is_partial_but_foreign_or_changed_structure_is_not_empty_success() {
    let (_http, client) = fixture(
        "",
        ResponseTemplate::new(200).set_body_string(page("", "", 0)),
        false,
        HttpSettings::default(),
    )
    .await;
    let empty = client
        .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
        .await
        .unwrap();
    assert!(empty.comments.is_empty());
    assert!(!empty.coverage.complete);
    for body in [
        "<html>different page</html>".into(),
        page("", &captured(""), 10).replace("/pull/1", "/pull/2"),
    ] {
        let (_http, client) = fixture(
            "",
            ResponseTemplate::new(200).set_body_string(body),
            false,
            HttpSettings::default(),
        )
        .await;
        assert!(matches!(
            client
                .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
                .await,
            Err(GbMcpError::Other(_))
        ));
    }
}

#[tokio::test]
async fn observed_head_change_returns_read_conflict_without_partial_data() {
    let (_http, client) = fixture(
        "",
        ResponseTemplate::new(200).set_body_string(page("", &captured(""), 10)),
        true,
        HttpSettings::default(),
    )
    .await;
    assert!(matches!(
        client
            .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
            .await,
        Err(GbMcpError::ReadConflict(_))
    ));
}

#[tokio::test]
async fn unselected_profile_missing_credentials_and_invalid_ids_send_no_http() {
    let server = MockServer::start().await;
    let rest = GitBucketClient::new_with_web_auth(
        &server.uri(),
        "fake",
        false,
        Some("user"),
        Some("pass"),
    )
    .unwrap();
    assert!(matches!(
        rest.list_pull_request_review_comments("owner", "repo", 1)
            .await,
        Err(GbMcpError::Unsupported(_))
    ));
    let standard = GitBucketClient::new(&server.uri(), "fake")
        .unwrap()
        .with_api_profile(ApiProfile::Standard);
    assert!(matches!(
        standard
            .list_pull_request_review_comments("owner", "repo", 1)
            .await,
        Err(GbMcpError::Authentication(_))
    ));
    for id in [
        "3",
        "issue:3",
        "commit:0",
        "commit:01",
        "commit:-1",
        "commit:3/4",
        "commit:18446744073709551616",
        " commit:3",
    ] {
        assert!(matches!(
            standard
                .get_pull_request_review_comment("owner", "repo", 1, id)
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert!(matches!(
        standard
            .list_pull_request_review_comments("owner", "repo", 0)
            .await,
        Err(GbMcpError::InvalidInput(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn mismatched_pat_or_cookie_identity_never_reads_the_review_page() {
    for wrong_pat in [true, false] {
        let (http, client) = fixture(
            "",
            ResponseTemplate::new(200).set_body_string(page("", "", 0)),
            false,
            HttpSettings::default(),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/user"))
            .respond_with(move |request: &wiremock::Request| {
                let pat = request.headers.contains_key("authorization");
                let login = if pat == wrong_pat {
                    "other-user"
                } else {
                    "probe-owner"
                };
                ResponseTemplate::new(200).set_body_json(json!({"login":login}))
            })
            .with_priority(1)
            .mount(&http)
            .await;
        assert!(matches!(
            client
                .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
                .await,
            Err(GbMcpError::Authentication(_))
        ));
        let requests = http.received_requests().await.unwrap();
        assert!(!requests
            .iter()
            .any(|r| r.url.path() == "/probe-owner/probe-repo/pull/1"));
        if wrong_pat {
            assert!(!requests.iter().any(|r| r.method == "POST"));
        }
    }
}

#[tokio::test]
async fn authentication_permissions_redirects_and_response_limits_fail_without_data() {
    for (response, expected) in [
        (
            ResponseTemplate::new(200).set_body_string(
                "<form action='/signin'><input type='password' value='fake-secret'></form>",
            ),
            "auth",
        ),
        (
            ResponseTemplate::new(403).set_body_string("fake-secret permission error"),
            "403",
        ),
        (
            ResponseTemplate::new(404).set_body_string("fake-secret missing"),
            "404",
        ),
        (
            ResponseTemplate::new(303).insert_header("Location", "/signin"),
            "auth",
        ),
        (
            ResponseTemplate::new(307).insert_header("Location", "https://foreign.test/secret"),
            "redirect",
        ),
    ] {
        let (http, client) = fixture("", response, false, HttpSettings::default()).await;
        let error = client
            .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("fake-secret"));
        match expected {
            "auth" => assert!(matches!(error, GbMcpError::Authentication(_))),
            "403" => assert!(matches!(error, GbMcpError::Api { status: 403, .. })),
            "404" => assert!(matches!(error, GbMcpError::Api { status: 404, .. })),
            _ => assert!(!error.to_string().contains("foreign.test")),
        }
        assert_eq!(
            http.received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.url.path() == "/signin")
                .count(),
            1
        );
    }
    let settings = HttpSettings {
        html_limit_bytes: 128,
        ..Default::default()
    };
    let (_http, client) = fixture(
        "",
        ResponseTemplate::new(200).set_body_string("x".repeat(129)),
        false,
        settings,
    )
    .await;
    assert!(matches!(
        client
            .list_pull_request_review_comments("probe-owner", "probe-repo", 1)
            .await,
        Err(GbMcpError::ResponseTooLarge { .. })
    ));
}

async fn mcp(server: GitBucketMcpServer) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (a, b) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let server = server.serve(a).await.unwrap();
        let _ = server.waiting().await;
    });
    ().serve(b).await.unwrap()
}

#[tokio::test]
async fn readonly_mcp_tools_return_coverage_and_namespaced_lookup_results() {
    let (_http, api) = fixture(
        "",
        ResponseTemplate::new(200).set_body_string(page("", &captured(""), 10)),
        false,
        HttpSettings::default(),
    )
    .await;
    let client = mcp(GitBucketMcpServer::new(api).with_access_mode(AccessMode::ReadOnly)).await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 19);
    for (name, extra) in [
        ("list_pull_request_review_comments", json!({})),
        (
            "get_pull_request_review_comment",
            json!({"comment_id":"commit:3"}),
        ),
    ] {
        let tool = tools.iter().find(|tool| tool.name == name).unwrap();
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(true)
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let mut args = json!({"owner":" probe-owner ","repo":" probe-repo ","pull_number":1});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = client
            .call_tool(
                CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        let data = result.structured_content.unwrap();
        assert_eq!(data["coverage"]["complete"], false);
        if name.starts_with("list_") {
            assert_eq!(data["comments"].as_array().unwrap().len(), 11);
        } else {
            assert_eq!(data["status"], "found");
            assert_eq!(data["comment"]["id"], "commit:3");
        }
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn review_continuations_keep_partial_coverage_and_do_not_repeat_web_reads() {
    let (http, api) = fixture(
        "",
        ResponseTemplate::new(200).set_body_string(page("", &captured(""), 10)),
        false,
        HttpSettings::default(),
    )
    .await;
    let client = mcp(GitBucketMcpServer::new(api).with_access_mode(AccessMode::ReadOnly)).await;
    let mut args =
        json!({"owner":"probe-owner","repo":"probe-repo","pull_number":1,"view":"full","limit":3});
    let mut coverage = None;
    let mut ids = std::collections::BTreeSet::new();
    let mut initial_reads = 0;
    loop {
        let result = client
            .call_tool(
                CallToolRequestParams::new("list_pull_request_review_comments")
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(serde_json::to_vec(&result).unwrap().len() <= 65536);
        let page = result.structured_content.unwrap();
        assert_eq!(page["range"]["source_complete"], false);
        assert_eq!(page["coverage"]["complete"], false);
        if let Some(first) = &coverage {
            assert_eq!(&page["coverage"], first);
            assert_eq!(http.received_requests().await.unwrap().len(), initial_reads);
        } else {
            coverage = Some(page["coverage"].clone());
            initial_reads = http.received_requests().await.unwrap().len();
        }
        for comment in page["comments"].as_array().unwrap() {
            assert!(ids.insert(comment["id"].as_str().unwrap().to_owned()));
            assert_eq!(comment["body_format"], "html");
        }
        if page["has_more"] == false {
            assert!(page["next_cursor"].is_null());
            break;
        }
        args["cursor"] = page["next_cursor"].clone();
    }
    assert_eq!(ids.len(), 11);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn review_tool_deadline_is_a_read_timeout_without_write_outcome() {
    let (_http, api) = fixture(
        "",
        ResponseTemplate::new(200)
            .set_body_string(page("", "", 0))
            .set_delay(Duration::from_millis(250)),
        false,
        HttpSettings::default(),
    )
    .await;
    let client = mcp(GitBucketMcpServer::with_api_and_timeout(
        Arc::new(api),
        Duration::from_millis(150),
    )
    .unwrap())
    .await;
    let result = client
        .call_tool(
            CallToolRequestParams::new("list_pull_request_review_comments").with_arguments(
                json!({"owner":"probe-owner","repo":"probe-repo","pull_number":1})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let error = result.structured_content.unwrap();
    assert_eq!(error["kind"], "timeout_error");
    assert!(error.get("operation").is_none());
    client.cancel().await.unwrap();
}
