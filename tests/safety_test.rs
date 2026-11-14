#[allow(dead_code)]
mod common;
use gitbucket_mcp_server::{
    api::client::{normalize_base_url, GitBucketClient},
    config::{Config, ConfigFile},
    error::GbMcpError,
    models::{issue::CreateIssue, milestone::UpdateMilestone},
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const SECRET: &str = "fake-private-token-password-123";

fn assert_redacted(value: impl std::error::Error) {
    assert!(!format!("{value:?} {value}").contains(SECRET));
    let mut source = value.source();
    while let Some(cause) = source {
        assert!(!format!("{cause:?} {cause}").contains(SECRET));
        source = cause.source();
    }
}

#[test]
fn base_urls_preserve_ipv6_and_context_but_reject_ambiguous_targets() {
    assert_eq!(
        normalize_base_url("http://[::1]:8080/team/gitbucket/").unwrap(),
        "http://[::1]:8080/team/gitbucket/api/v3"
    );
    assert_eq!(
        normalize_base_url("https://example.test/%E3%81%82").unwrap(),
        "https://example.test/%E3%81%82/api/v3"
    );
    for url in [
        "https:///example.test",
        "https://example.test/%00",
        "https://example.test/%zz",
        "ftp://example.test",
        "file:///tmp/gb",
        "https://user:password@example.test",
        "https://@example.test",
        "https://example.test/?token=secret",
        "https://example.test/#secret",
        "https://example.test/a/../b",
        "https://example.test/a/%2E%2E/b",
        "https://example.test/a%2fb",
        "https://example.test/a%252fb",
        "https://example.test/a\\b",
        "https://example.test/a\nb",
    ] {
        assert!(normalize_base_url(url).is_err(), "accepted {url}");
    }
}

#[tokio::test]
async fn invalid_identifiers_never_reach_any_http_target() {
    let server = MockServer::start().await;
    let client = GitBucketClient::new(&server.uri(), SECRET).unwrap();
    for bad in [
        "r#different",
        "r?query",
        "a/b",
        "..",
        ".",
        "%2e%2e",
        "-name",
        "_name",
        "r%23different",
        "a\\b",
        "a\nb",
        "日本語",
        "",
    ] {
        assert!(matches!(
            client.get_repository("owner", bad).await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client.get_user(bad).await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client.list_repositories(bad).await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client
                .create_issue(
                    bad,
                    "repo",
                    &CreateIssue {
                        milestone: None,
                        title: "test".into(),
                        body: None,
                        assignees: None,
                        labels: None
                    }
                )
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client
                .update_milestone(
                    "owner",
                    bad,
                    1,
                    &UpdateMilestone {
                        title: Some("test".into()),
                        description: None,
                        state: None,
                        due_on: None
                    }
                )
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
        assert!(matches!(
            client.delete_label(bad, "repo", "label").await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn valid_identifiers_and_special_label_names_preserve_exact_targets() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/context/api/v3/repos/user-name_1/repo.name+2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":1,"name":"repo.name+2","full_name":"user-name_1/repo.name+2","private":false}))).mount(&server).await;
    let client = GitBucketClient::new(&format!("{}/context", server.uri()), SECRET).unwrap();
    client
        .get_repository("user-name_1", "repo.name+2")
        .await
        .unwrap();
    // Labels have a different grammar: slashes, spaces, Unicode and URL delimiters
    // belong to one encoded segment, rather than to the request URL syntax.
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    client
        .delete_label("user-name_1", "repo.name+2", "日本語 /#?%")
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let url = &requests[1].url;
    assert!(
        url.path()
            .ends_with("/labels/%E6%97%A5%E6%9C%AC%E8%AA%9E%20%2F%23%3F%25"),
        "{url}"
    );
    assert!(url.query().is_none());
    assert!(url.fragment().is_none());
    for bad in [".", ".."] {
        assert!(client.delete_label("owner", "repo", bad).await.is_err());
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn issue_close_action_reaches_the_intended_comment_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/context/api/v3/repos/owner/repo/issues/7/comments"))
        .and(wiremock::matchers::query_param("action", "close"))
        .and(wiremock::matchers::query_param("note", "日本語 /#?%"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id":1})))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitBucketClient::new(&format!("{}/context", server.uri()), SECRET).unwrap();
    let comment: Value = client
        .post_with_query(
            "/repos/owner/repo/issues/7/comments",
            &[("action", "close"), ("note", "日本語 /#?%")],
            &json!({"body":"Close disposable list fixture"}),
        )
        .await
        .unwrap();
    assert_eq!(comment["id"], 1);
    for unsafe_path in [
        "/comments?action=close",
        "/comments#other",
        "/comments%2fother",
    ] {
        assert!(matches!(
            client
                .post_with_query::<Value, _>(unsafe_path, &[("action", "close")], &json!({}))
                .await,
            Err(GbMcpError::InvalidInput(_))
        ));
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.query_pairs().count(), 2);
    assert!(requests[0].url.fragment().is_none());
}

#[tokio::test]
async fn api_error_payloads_and_invalid_json_do_not_echo_secrets() {
    let server = MockServer::start().await;
    let client = GitBucketClient::new(&server.uri(), SECRET).unwrap();
    for verb in ["GET", "DELETE", "POST", "PUT", "PATCH"] {
        Mock::given(method(verb))
            .and(path("/api/v3/failure"))
            .respond_with(ResponseTemplate::new(403).set_body_string(SECRET.repeat(10000)))
            .mount(&server)
            .await;
    }
    let failures = [
        client.get::<Value>("/failure").await.unwrap_err(),
        client.delete("/failure").await.unwrap_err(),
        client
            .post::<Value, _>("/failure", &json!({}))
            .await
            .unwrap_err(),
        client
            .put::<Value, _>("/failure", &json!({}))
            .await
            .unwrap_err(),
        client
            .patch::<Value, _>("/failure", &json!({}))
            .await
            .unwrap_err(),
    ];
    for error in failures {
        assert_redacted(error);
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/wrapper"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status":422,"message":SECRET,"body":SECRET})),
        )
        .mount(&server)
        .await;
    assert_redacted(client.get::<Value>("/wrapper").await.unwrap_err());
    Mock::given(method("GET"))
        .and(path("/api/v3/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(SECRET)))
        .mount(&server)
        .await;
    assert_redacted(client.get::<u64>("/json").await.unwrap_err());
}

#[tokio::test]
async fn json_error_sources_do_not_echo_remote_field_values() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "login":"user", "site_admin":SECRET
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitBucketClient::new(&server.uri(), SECRET).unwrap();
    let error = client.get_user("user").await.unwrap_err();
    assert!(matches!(error, GbMcpError::Json(_)));
    assert_redacted(error);
}

#[tokio::test]
async fn redirects_cannot_send_credentials_outside_origin_or_context() {
    let source = MockServer::start().await;
    let target = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&target)
        .await;
    Mock::given(method("GET"))
        .and(path("/context/api/v3/redirect"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "Location",
            format!("{}/{}?token={SECRET}", target.uri(), SECRET),
        ))
        .mount(&source)
        .await;
    common::web_identity(&source, "/context", "test-user").await;
    let client = GitBucketClient::new(&format!("{}/context", source.uri()), SECRET).unwrap();
    assert_redacted(client.get::<Value>("/redirect").await.unwrap_err());
    Mock::given(method("GET"))
        .and(path("/context/api/v3/escape"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/elsewhere"))
        .mount(&source)
        .await;
    assert!(client.get::<Value>("/escape").await.is_err());
    Mock::given(method("POST"))
        .and(path("/context/signin"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("Location", format!("{}/signin", target.uri())),
        )
        .mount(&source)
        .await;
    assert_redacted(
        GitBucketClient::new_with_web_auth(
            &format!("{}/context/api/v3", source.uri()),
            SECRET,
            false,
            Some("test-user"),
            Some(SECRET),
        )
        .unwrap()
        .web_session()
        .await
        .unwrap_err(),
    );
    assert!(target.received_requests().await.unwrap().is_empty());
    Mock::given(method("GET"))
        .and(path("/context/api/v3/encoded-escape"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", "/context/%2F..%2Felsewhere"),
        )
        .mount(&source)
        .await;
    assert!(client.get::<Value>("/encoded-escape").await.is_err());
    assert_eq!(source.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn web_errors_do_not_echo_payloads_and_invalid_targets_are_rejected() {
    let server = MockServer::start().await;
    common::web_identity(&server, "/ctx", "user").await;
    Mock::given(method("POST"))
        .and(path("/ctx/signin"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/ctx/")
                .insert_header("Set-Cookie", "JSESSIONID=test; Path=/"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ctx/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let web = GitBucketClient::new_with_web_auth(
        &format!("{}/ctx/api/v3", server.uri()),
        SECRET,
        false,
        Some("user"),
        Some(SECRET),
    )
    .unwrap()
    .web_session()
    .await
    .unwrap();
    assert!(!format!("{web:?}").contains(SECRET));
    Mock::given(method("POST"))
        .and(path("/ctx/owner/repo/issues/edit_title/1"))
        .respond_with(ResponseTemplate::new(500).set_body_string(SECRET))
        .mount(&server)
        .await;
    assert_redacted(
        web.edit_issue_title("owner", "repo", 1, "title")
            .await
            .unwrap_err(),
    );
    let count = server.received_requests().await.unwrap().len();
    assert!(web
        .update_milestone_state("owner", "repo#other", 1, "open")
        .await
        .is_err());
    assert!(web
        .edit_issue_title("../owner", "repo", 1, "title")
        .await
        .is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), count);
}

#[tokio::test]
async fn web_redirects_cannot_escape_a_context_ending_in_the_api_suffix() {
    for status in [307, 308] {
        let server = MockServer::start().await;
        common::web_identity(&server, "/team/api/v3", "test-user").await;
        Mock::given(method("POST"))
            .and(path("/team/api/v3/signin"))
            .respond_with(
                ResponseTemplate::new(status).insert_header("Location", "/team/collector"),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/team/collector"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let result = GitBucketClient::new_with_web_auth(
            &format!("{}/team/api/v3/api/v3", server.uri()),
            SECRET,
            false,
            Some("test-user"),
            Some(SECRET),
        )
        .unwrap()
        .web_session()
        .await;
        assert!(result.is_err(), "out-of-context HTTP {status} was followed");
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn web_redirects_preserve_contexts_ending_in_the_api_suffix() {
    for context in ["/team/api/v3", "/team/api/v3/api/v3"] {
        let server = MockServer::start().await;
        common::web_identity(&server, context, "test-user").await;
        Mock::given(method("GET"))
            .and(path(format!("{context}/owner/repo/issues/_data/1")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"title":"title","content":"body"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{context}/signin")))
            .respond_with(
                ResponseTemplate::new(303)
                    .insert_header("Location", format!("{context}/dashboard"))
                    .insert_header("Set-Cookie", "JSESSIONID=nested-context; Path=/"),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{context}/dashboard")))
            .and(wiremock::matchers::header(
                "cookie",
                "JSESSIONID=nested-context",
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{context}/owner/repo/issues/edit_title/1")))
            .respond_with(
                ResponseTemplate::new(303)
                    .insert_header("Location", format!("{context}/owner/repo/issues/_data/1")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let web = GitBucketClient::new_with_web_auth(
            &format!("{}{context}/api/v3", server.uri()),
            SECRET,
            false,
            Some("test-user"),
            Some(SECRET),
        )
        .unwrap()
        .web_session()
        .await
        .unwrap();
        web.edit_issue_title("owner", "repo", 1, "title")
            .await
            .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 7);
    }
}

#[test]
fn config_debug_and_parse_diagnostics_redact_values() {
    let config = Config {
        gitbucket_url: format!("https://user:{SECRET}@example.test"),
        gitbucket_token: SECRET.into(),
        gitbucket_username: Some(SECRET.into()),
        gitbucket_password: Some(SECRET.into()),
    };
    assert!(!format!("{config:?}").contains(SECRET));
    let file = ConfigFile {
        url: Some(SECRET.into()),
        token: Some(SECRET.into()),
        username: Some(SECRET.into()),
        password: Some(SECRET.into()),
    };
    assert!(!format!("{file:?}").contains(SECRET));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    for content in [
        format!("token = \"{SECRET}"),
        format!("token = [\"{SECRET}\"]"),
        format!("{SECRET} = [broken"),
    ] {
        std::fs::write(&path, content).unwrap();
        assert_redacted(ConfigFile::load_from(&path).unwrap_err());
    }
    let client = GitBucketClient::new_with_web_auth(
        "https://example.test",
        SECRET,
        false,
        Some(SECRET),
        Some(SECRET),
    )
    .unwrap();
    assert!(!format!("{client:?}").contains(SECRET));
}

#[cfg(unix)]
#[test]
fn config_save_is_private_atomic_and_does_not_follow_links() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let config = ConfigFile {
        url: Some("https://example.test".into()),
        token: Some(SECRET.into()),
        ..Default::default()
    };
    config.save_to(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let old_file = std::fs::File::open(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    config.save_to(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    use std::os::unix::fs::MetadataExt;
    assert_ne!(
        old_file.metadata().unwrap().ino(),
        std::fs::metadata(&path).unwrap().ino()
    );
    let link = dir.path().join("link.toml");
    symlink(&path, &link).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(config.save_to(&link).is_err());
    assert!(ConfigFile::load_from(&link).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let dangling = dir.path().join("dangling.toml");
    symlink(dir.path().join("missing"), &dangling).unwrap();
    assert!(config.save_to(&dangling).is_err());
    assert!(ConfigFile::load_from(&dangling).is_err());
    assert!(config.save_to(dir.path()).is_err());
    // Invalid destinations must leave no temporary residue.
    let too_long = dir.path().join("x".repeat(300));
    assert!(config.save_to(&too_long).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    // The existing secret remains intact if a replacement cannot be created.
    if unsafe { libc::geteuid() } != 0 {
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = config.save_to(&path);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}

#[test]
fn startup_stderr_never_contains_malformed_toml_or_invalid_url_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    for (contents, url) in [
        (format!("token = \"{SECRET}"), "https://example.test".into()),
        (String::new(), format!("https://user:{SECRET}@example.test")),
    ] {
        std::fs::write(&config, &contents).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gitbucket-mcp-server"));
        command
            .env_clear()
            .env("GITBUCKET_MCP_CONFIG_DIR", dir.path())
            .env("GITBUCKET_URL", url);
        // A needed malformed file must fail; complete env deliberately skips it.
        if contents.is_empty() {
            command.env("GITBUCKET_TOKEN", SECRET);
        }
        let result = command.output().unwrap();
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains(SECRET));
        assert!(result.stdout.is_empty());
    }
}

#[tokio::test]
async fn tool_errors_keep_classification_without_echoing_remote_data_or_url() {
    use gitbucket_mcp_server::{server::GitBucketMcpServer, tools::user::GetUserParams};
    use rmcp::handler::server::wrapper::Parameters;
    let http = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/users/{SECRET}")))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "Location",
            format!("http://127.0.0.1:1/{SECRET}?token={SECRET}"),
        ))
        .mount(&http)
        .await;
    let client = GitBucketClient::new(&http.uri(), SECRET).unwrap();
    let server = GitBucketMcpServer::new(client);
    let result = server
        .get_user(Parameters(GetUserParams {
            username: SECRET.into(),
        }))
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.as_ref().unwrap()["kind"],
        "http_error"
    );
    assert!(!format!("{result:?}").contains(SECRET));
    let count = http.received_requests().await.unwrap().len();
    let result = server
        .get_user(Parameters(GetUserParams {
            username: "owner#other".into(),
        }))
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.as_ref().unwrap()["kind"],
        "validation_error"
    );
    assert_eq!(http.received_requests().await.unwrap().len(), count);
    Mock::given(method("GET"))
        .and(path("/api/v3/users/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"login":SECRET,"site_admin":SECRET})),
        )
        .mount(&http)
        .await;
    let result = server
        .get_user(Parameters(GetUserParams {
            username: "user".into(),
        }))
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.as_ref().unwrap()["kind"],
        "json_error"
    );
    assert!(!format!("{result:?}").contains(SECRET));
}

#[cfg(unix)]
#[test]
fn permissive_config_load_warns_without_changing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        format!("url = \"https://example.test\"\ntoken = \"{SECRET}\"\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_gitbucket-mcp-server"))
        .env_clear()
        .env("GITBUCKET_MCP_CONFIG_DIR", dir.path())
        .env("GITBUCKET_URL", "ftp://example.test")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("chmod 600"));
    assert!(!stderr.contains(SECRET));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}
