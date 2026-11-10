use gitbucket_mcp_server::{
    api::{client::GitBucketClient, http::HttpSettings, profile::ApiProfile},
    error::GbMcpError,
    server::{AccessMode, GitBucketMcpServer},
};
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
fn pr(head: &str) -> Value {
    json!({"number":1,"title":"PR","state":"open","head":{"ref":"feature","sha":head},"base":{"ref":"main","sha":BASE}})
}
fn commit(sha: &str, message: &str) -> Value {
    json!({"sha":sha,"commit":{"message":message,"author":{"name":"Author","email":"author@example.test","date":"2026-01-01T00:00:00Z"},"committer":{"name":"Committer","email":"committer@example.test","date":"2026-01-02T00:00:00Z"},"url":"https://example.test/git/commits"},"author":null,"committer":null,"parents":[{"sha":BASE}],"url":"https://example.test/commits"})
}
fn patch(path: &str, text: &str) -> String {
    format!("diff --git a/{path} b/{path}\nindex 123..456 100644\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+{text}\n")
}
async fn fixture(body: String, race: bool) -> (MockServer, GitBucketClient) {
    let http = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(pr(
                if race && count.fetch_add(1, Ordering::SeqCst) > 0 {
                    BASE
                } else {
                    HEAD
                },
            ))
        })
        .mount(&http)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"alice"})))
        .mount(&http)
        .await;
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("Location", "/")
                .insert_header("Set-Cookie", "JSESSIONID=fake; Path=/"),
        )
        .mount(&http)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html>Authenticated landing</html>"),
        )
        .mount(&http)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/owner/repo/patch/{BASE}...{HEAD}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/plain")
                .set_body_string(body),
        )
        .mount(&http)
        .await;
    let api = GitBucketClient::new_with_web_auth(
        &http.uri(),
        "fake-token",
        false,
        Some("alice"),
        Some("fake-password"),
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard);
    (http, api)
}
async fn connect(api: GitBucketClient) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (server, client) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let service = GitBucketMcpServer::new(api)
            .with_access_mode(AccessMode::ReadOnly)
            .serve(server)
            .await
            .unwrap();
        let _ = service.waiting().await;
    });
    ().serve(client).await.unwrap()
}
async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    args: Value,
) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 64 * 1024);
    assert_eq!(result.is_error, Some(false), "{result:?}");
    result.structured_content.unwrap()
}

#[tokio::test]
async fn commits_are_unpaged_pat_only_preserve_full_message_and_unknown_accounts() {
    let (http, api) = fixture(String::new(), false).await;
    let message = "subject\n\n    code\nhard break  \n日本語\n";
    let mut commits = vec![commit(HEAD, message)];
    commits.extend((1..=52).map(|i| commit(&format!("{i:040x}"), "message")));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commits))
        .expect(1)
        .mount(&http)
        .await;
    let pat = GitBucketClient::new(&http.uri(), "fake-token").unwrap();
    let observed = pat
        .list_pull_request_commits("owner", "repo", 1)
        .await
        .unwrap();
    assert_eq!(observed.commits.len(), 53);
    assert_eq!(observed.commits[0].commit.message, message);
    assert!(observed.commits[0].author.is_none());
    assert!(!observed.comparison.atomic);
    let requests = http.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests
        .iter()
        .all(|r| r.method == "GET" && r.url.query().is_none()));
    assert_eq!(api.api_profile(), ApiProfile::Standard);
}
#[tokio::test]
async fn invalid_commit_identity_duplicates_missing_head_and_pr_races_are_errors() {
    for payload in [
        json!([commit(HEAD, "x"), commit(HEAD, "x")]),
        json!([commit(BASE, "x")]),
        json!([commit("invalid", "x")]),
    ] {
        let (http, api) = fixture(String::new(), false).await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/pulls/1/commits"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .mount(&http)
            .await;
        assert!(api
            .list_pull_request_commits("owner", "repo", 1)
            .await
            .is_err());
    }
    let (http, api) = fixture(String::new(), true).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![commit(HEAD, "x")]))
        .mount(&http)
        .await;
    assert!(matches!(
        api.list_pull_request_commits("owner", "repo", 1).await,
        Err(GbMcpError::ReadConflict(_))
    ));
}
#[tokio::test]
async fn profiles_credentials_and_invalid_inputs_fail_before_http() {
    let http = MockServer::start().await;
    let pat = GitBucketClient::new(&http.uri(), "fake").unwrap();
    assert!(matches!(
        pat.list_pull_request_files("owner", "repo", 1).await,
        Err(GbMcpError::Unsupported(_))
    ));
    assert!(matches!(
        pat.clone()
            .with_api_profile(ApiProfile::Standard)
            .list_pull_request_files("owner", "repo", 1)
            .await,
        Err(GbMcpError::Authentication(_))
    ));
    assert!(pat
        .list_pull_request_commits("owner", "repo", 0)
        .await
        .is_err());
    assert!(pat
        .get_pull_request_diff("owner", "repo", 1, Some(""))
        .await
        .is_err());
    assert!(http.received_requests().await.unwrap().is_empty());
}
#[tokio::test]
async fn complete_file_metadata_and_raw_chunks_preserve_binary_delete_mode_and_unicode() {
    let rename="diff --git a/old name b/new name\nsimilarity index 100%\nrename from old name\nrename to new name\n";
    let binary = "diff --git a/image.bin b/image.bin\nindex 123..456 100644\nBinary files differ\n";
    let deleted="diff --git a/gone b/gone\ndeleted file mode 100644\nindex 123..000\n--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n";
    let mode = "diff --git a/script b/script\nold mode 100644\nnew mode 100755\n";
    let raw = format!(
        "{}{rename}{binary}{deleted}{mode}",
        patch("日本 & file.txt", &"日本語".repeat(2000))
    );
    let (_http, api) = fixture(raw.clone(), false).await;
    let data = api
        .list_pull_request_files("owner", "repo", 1)
        .await
        .unwrap();
    assert_eq!(data.files.len(), 5);
    assert_eq!(data.files[0].additions, Some(1));
    assert_eq!(
        data.files[1].status,
        gitbucket_mcp_server::models::pull_request_changes::FileChangeStatus::Renamed
    );
    assert!(data.files[2].binary);
    assert_eq!(data.files[2].additions, None);
    assert_eq!(data.files[3].new_path, None);
    assert_eq!(data.files[4].hunks, 0);
    let d = api
        .get_pull_request_diff("owner", "repo", 1, None)
        .await
        .unwrap();
    assert_eq!(d.total_patch_bytes, raw.len());
    assert_eq!(
        d.chunks.iter().map(|c| c.text.as_str()).collect::<String>(),
        raw
    );
    assert!(d
        .chunks
        .iter()
        .all(|c| c.text.len() <= 4096 && c.end_byte - c.start_byte == c.text.len()));
    let single = api
        .get_pull_request_diff("owner", "repo", 1, Some("old name"))
        .await
        .unwrap();
    assert_eq!(single.selected_files, 1);
    assert_eq!(
        single
            .chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>(),
        rename
    );
    assert!(api
        .get_pull_request_diff("owner", "repo", 1, Some(" name "))
        .await
        .is_err());
}
#[tokio::test]
async fn malformed_sources_races_and_http_body_bounds_never_return_partial_success() {
    for raw in [
        "diff --git a/x b/x\n",
        "not a patch",
        "diff --git a/x b/x\nindex abc..def 100644\n",
        "diff --git a/x b/x\nindex abc..def 100644\n--- a/x\n+++ b/x\n@@ -1,2 +1 @@\n-old\n+new\n",
    ] {
        let (_http, api) = fixture(raw.into(), false).await;
        assert!(api
            .list_pull_request_files("owner", "repo", 1)
            .await
            .is_err());
    }
    let (_http, api) = fixture(patch("x", "new"), true).await;
    assert!(matches!(
        api.list_pull_request_files("owner", "repo", 1).await,
        Err(GbMcpError::ReadConflict(_))
    ));
    let (http, _) = fixture(patch("x", &"big".repeat(1000)), false).await;
    let api = GitBucketClient::new_with_web_auth_and_http_settings(
        &http.uri(),
        "fake",
        false,
        Some("alice"),
        Some("fake"),
        HttpSettings {
            html_limit_bytes: 500,
            ..Default::default()
        },
    )
    .unwrap()
    .with_api_profile(ApiProfile::Standard);
    assert!(matches!(
        api.list_pull_request_files("owner", "repo", 1).await,
        Err(GbMcpError::ResponseTooLarge { .. })
    ));
}
#[tokio::test]
async fn truncated_change_blocks_and_unterminated_patch_lines_are_errors() {
    let added = "diff --git a/new.txt b/new.txt\nnew file mode 100644\nindex 0000000..3e75765\n";
    let deleted =
        "diff --git a/old.txt b/old.txt\ndeleted file mode 100644\nindex 3367afd..0000000\n";
    let renamed = "diff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\nindex 3367afd..3e75765 100644\n";
    let renamed_before_index = renamed.split("index 3367afd").next().unwrap();
    let mode_and_content =
        "diff --git a/script b/script\nold mode 100644\nnew mode 100755\nindex 3367afd..3e75765\n";
    let unterminated = patch("x", "new").trim_end_matches('\n').to_owned();
    for raw in [
        added,
        deleted,
        renamed,
        renamed_before_index,
        mode_and_content,
        &unterminated,
    ] {
        let (_http, api) = fixture(raw.into(), false).await;
        assert!(
            matches!(
                api.list_pull_request_files("owner", "repo", 1).await,
                Err(GbMcpError::Unsupported(_))
            ),
            "truncated file list succeeded for {raw:?}"
        );
        assert!(
            matches!(
                api.get_pull_request_diff("owner", "repo", 1, None).await,
                Err(GbMcpError::Unsupported(_))
            ),
            "truncated diff succeeded for {raw:?}"
        );
    }
}
#[tokio::test]
async fn commit_cursor_is_cached_bound_to_tool_target_and_view_and_summary_is_explicit() {
    let (http, api) = fixture(String::new(), false).await;
    let message = "日本語".repeat(2000);
    let mut commits = vec![commit(HEAD, &message)];
    commits.extend((1..=52).map(|i| commit(&format!("{i:040x}"), "x")));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commits))
        .expect(2)
        .mount(&http)
        .await;
    let client = connect(api).await;
    let args = json!({"owner":"owner","repo":"repo","pull_number":1,"limit":10});
    let mut page = call(&client, "list_pull_request_commits", args.clone()).await;
    assert!(
        page["commits"][0]["commit"]["message"]
            .as_str()
            .unwrap()
            .len()
            <= 1024
    );
    assert!(page["commits"][0]["_list"]["truncated_fields"]
        .as_array()
        .unwrap()
        .contains(&json!("/commit/message")));
    let before = http.received_requests().await.unwrap().len();
    let mut total = 10;
    let cursor = page["next_cursor"].clone();
    for wrong in [
        json!({"view":"full"}),
        json!({"pull_number":2}),
        json!({"limit":9}),
    ] {
        let mut invalid = args.clone();
        invalid["cursor"] = cursor.clone();
        for (k, v) in wrong.as_object().unwrap() {
            invalid[k] = v.clone();
        }
        let result = client
            .call_tool(
                CallToolRequestParams::new("list_pull_request_commits")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }
    while page["has_more"] == true {
        let mut next = args.clone();
        next["cursor"] = page["next_cursor"].clone();
        page = call(&client, "list_pull_request_commits", next).await;
        total += page["commits"].as_array().unwrap().len();
    }
    assert_eq!(total, 53);
    assert_eq!(http.received_requests().await.unwrap().len(), before);
    let mut full = args;
    full["view"] = json!("full");
    assert_eq!(
        call(&client, "list_pull_request_commits", full).await["commits"][0]["commit"]["message"],
        message
    );
    client.cancel().await.unwrap();
}
#[tokio::test]
async fn diff_continuations_reconstruct_exact_source_under_byte_bound_and_reject_path_rebinding() {
    let raw = patch("日本 file.txt", &"日本語\\quote\"".repeat(6000));
    let (http, api) = fixture(raw.clone(), false).await;
    let client = connect(api).await;
    let args = json!({"owner":"owner","repo":"repo","pull_number":1,"limit":1});
    let mut page = call(&client, "get_pull_request_diff", args.clone()).await;
    assert_eq!(page["range"]["view"], "full");
    assert!(page["has_more"].as_bool().unwrap());
    let before = http.received_requests().await.unwrap().len();
    let mut wrong = args.clone();
    wrong["cursor"] = page["next_cursor"].clone();
    wrong["path"] = json!("日本 file.txt");
    let result = client
        .call_tool(
            CallToolRequestParams::new("get_pull_request_diff")
                .with_arguments(wrong.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    let mut text = String::new();
    let mut offset = 0;
    loop {
        for c in page["chunks"].as_array().unwrap() {
            assert_eq!(c["start_byte"], offset);
            let s = c["text"].as_str().unwrap();
            text.push_str(s);
            offset += s.len();
            assert_eq!(c["end_byte"], offset);
        }
        if page["has_more"] == false {
            break;
        }
        let mut next = args.clone();
        next["cursor"] = page["next_cursor"].clone();
        page = call(&client, "get_pull_request_diff", next).await;
    }
    assert_eq!(text, raw);
    assert_eq!(http.received_requests().await.unwrap().len(), before);
    client.cancel().await.unwrap();
}
