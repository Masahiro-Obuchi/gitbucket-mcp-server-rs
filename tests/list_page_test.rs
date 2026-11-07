mod common;

use common::TestServer;
use gitbucket_mcp_server::server::{AccessMode, GitBucketMcpServer};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    service::{RoleClient, RunningService},
    ServiceExt,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, ResponseTemplate,
};

async fn connect(server: GitBucketMcpServer) -> RunningService<RoleClient, ()> {
    let (a, b) = tokio::io::duplex(65536);
    tokio::spawn(async move {
        let running = server.serve(a).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(b).await.unwrap()
}
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, args: &Value) -> CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}
fn output(result: CallToolResult) -> Value {
    assert_eq!(
        result.is_error,
        Some(false),
        "{:?}",
        result.structured_content
    );
    assert!(serde_json::to_vec(&result).unwrap().len() <= 65536);
    result.structured_content.unwrap()
}
async fn collect(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    mut args: Value,
    key: &str,
) -> (Vec<Value>, Vec<Value>) {
    let mut all = Vec::new();
    let mut pages = Vec::new();
    let mut observation = None;
    loop {
        let page = output(call(client, tool, &args).await);
        assert_eq!(page["range"]["start"], all.len());
        assert_eq!(page["range"]["atomic"], false);
        if let Some(id) = &observation {
            assert_eq!(&page["range"]["observation_id"], id);
        } else {
            observation = Some(page["range"]["observation_id"].clone());
        }
        all.extend(page[key].as_array().unwrap().iter().cloned());
        assert_eq!(page["range"]["end"], all.len());
        let more = page["has_more"].as_bool().unwrap();
        args["cursor"] = page["next_cursor"].clone();
        pages.push(page);
        if !more {
            assert!(args["cursor"].is_null());
            break;
        }
        assert!(!pages.last().unwrap()[key].as_array().unwrap().is_empty());
    }
    assert_eq!(pages[0]["range"]["observed_total"], all.len());
    (all, pages)
}
async fn paged(http: &TestServer, endpoint: &str, rows: &[Value]) {
    for (index, chunk) in rows.chunks(25).enumerate() {
        Mock::given(method("GET"))
            .and(path(endpoint))
            .and(query_param("page", (index + 1).to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(chunk))
            .expect(1)
            .mount(&http.mock_server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(endpoint))
        .and(query_param(
            "page",
            (rows.len().div_ceil(25) + 1).to_string(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&http.mock_server)
        .await;
}

#[tokio::test]
async fn all_rest_list_tools_preserve_keys_and_page_without_refetching() {
    for (tool, endpoint, key, identity) in [
        (
            "list_repositories",
            "/api/v3/users/owner/repos",
            "repositories",
            "full_name",
        ),
        (
            "list_branches",
            "/api/v3/repos/owner/repo/branches",
            "branches",
            "name",
        ),
        (
            "list_labels",
            "/api/v3/repos/owner/repo/labels",
            "labels",
            "name",
        ),
        (
            "list_milestones",
            "/api/v3/repos/owner/repo/milestones",
            "milestones",
            "number",
        ),
        (
            "list_issues",
            "/api/v3/repos/owner/repo/issues",
            "issues",
            "number",
        ),
        (
            "list_pull_requests",
            "/api/v3/repos/owner/repo/pulls",
            "pull_requests",
            "number",
        ),
        (
            "list_issue_comments",
            "/api/v3/repos/owner/repo/issues/1/comments",
            "comments",
            "id",
        ),
    ] {
        let http = TestServer::start().await;
        let rows:Vec<_>=(1..=51).map(|i|match key {
            "repositories"=>json!({"name":format!("repo-{i}"),"full_name":format!("owner/repo-{i}"),"private":false}),
            "branches"|"labels"=>json!({"name":format!("entry-{i}")}),
            "comments"=>json!({"id":i,"body":"text"}),
            _=>json!({"number":i,"title":format!("Entry {i}"),"state":"open"}),
        }).collect();
        if matches!(key, "issues" | "pull_requests") {
            paged(&http, endpoint, &rows).await;
        } else {
            Mock::given(method("GET"))
                .and(path(endpoint))
                .respond_with(ResponseTemplate::new(200).set_body_json(&rows))
                .expect(1)
                .mount(&http.mock_server)
                .await;
        }
        let client =
            connect(GitBucketMcpServer::new(http.client()).with_access_mode(AccessMode::ReadOnly))
                .await;
        let mut args = json!({"owner":" owner ","limit":17});
        if key != "repositories" {
            args["repo"] = json!("repo");
        }
        if tool == "list_issue_comments" {
            args["issue_number"] = json!(1);
        }
        let (actual, pages) = collect(&client, tool, args, key).await;
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0]["range"]["limited_by"], "count");
        assert_eq!(pages[0]["range"]["source_complete"], true);
        assert_eq!(
            actual
                .iter()
                .map(|r| r[identity].to_string())
                .collect::<BTreeSet<_>>(),
            rows.iter().map(|r| r[identity].to_string()).collect()
        );
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn count_boundaries_and_default_limit_preserve_every_identity() {
    for count in [0, 1, 49, 50, 51, 99, 100, 101] {
        for limit in [None, Some(100)] {
            let http = TestServer::start().await;
            let rows: Vec<_> = (1..=count)
                .map(|i| json!({"number":i,"title":"Issue","state":"open"}))
                .collect();
            paged(&http, "/api/v3/repos/owner/repo/issues", &rows).await;
            let client = connect(GitBucketMcpServer::new(http.client())).await;
            let mut args = json!({"owner":"owner","repo":"repo"});
            if let Some(limit) = limit {
                args["limit"] = json!(limit);
            }
            let (items, pages) = collect(&client, "list_issues", args, "issues").await;
            assert_eq!(items.len(), count);
            assert_eq!(pages.len(), count.div_ceil(limit.unwrap_or(50)).max(1));
            assert_eq!(
                items
                    .iter()
                    .map(|r| r["number"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                (1..=count as u64).collect::<Vec<_>>()
            );
            client.cancel().await.unwrap();
        }
    }
}

#[tokio::test]
async fn cursors_are_issued_and_bound_to_scope_filters_view_limit_and_server() {
    let http = TestServer::start().await;
    let rows: Vec<_> = (1..=4).map(|i| json!({"id":i,"body":"observed"})).collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(rows))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let args = json!({"owner":"owner","repo":"repo","issue_number":1,"limit":1});
    let first = output(call(&client, "list_issue_comments", &args).await);
    let cursor = first["next_cursor"].as_str().unwrap();
    let mut normal = args.clone();
    normal["cursor"] = json!(cursor);
    for patch in [
        json!({"owner":"other"}),
        json!({"repo":"other"}),
        json!({"issue_number":2}),
        json!({"limit":2}),
        json!({"view":"full"}),
        json!({"cursor":"unknown"}),
        json!({"cursor":format!("{}.3",cursor.split('.').next().unwrap())}),
    ] {
        let mut invalid = normal.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            call(&client, "list_issue_comments", &invalid)
                .await
                .structured_content
                .unwrap()["kind"],
            "validation_error"
        );
    }
    let mut cross = normal.clone();
    cross.as_object_mut().unwrap().remove("issue_number");
    assert_eq!(
        call(&client, "list_branches", &cross)
            .await
            .structured_content
            .unwrap()["kind"],
        "validation_error"
    );
    let foreign = connect(GitBucketMcpServer::new(http.client())).await;
    assert_eq!(
        call(&foreign, "list_issue_comments", &normal)
            .await
            .structured_content
            .unwrap()["kind"],
        "validation_error"
    );
    let second = output(call(&client, "list_issue_comments", &normal).await);
    let replay = output(call(&client, "list_issue_comments", &normal).await);
    assert_eq!(second, replay);
    assert_eq!(second["comments"][0]["id"], 2);
    // Omitted state and explicit normalized open share a scope; other filters do not.
    let rows = vec![
        json!({"number":1,"title":"A","state":"open"}),
        json!({"number":2,"title":"B","state":"open"}),
    ];
    paged(&http, "/api/v3/repos/owner/repo/issues", &rows).await;
    let mut args = json!({"owner":"owner","repo":"repo","limit":1});
    let p = output(call(&client, "list_issues", &args).await);
    args["cursor"] = p["next_cursor"].clone();
    args["state"] = json!("closed");
    assert_eq!(
        call(&client, "list_issues", &args)
            .await
            .structured_content
            .unwrap()["kind"],
        "validation_error"
    );
    args["state"] = json!(" open ");
    assert_eq!(
        output(call(&client, "list_issues", &args).await)["issues"][0]["number"],
        2
    );
    foreign.cancel().await.unwrap();
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn invalid_limits_and_cursor_shapes_do_not_send_http() {
    let http = TestServer::start().await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    for patch in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"cursor":""}),
        json!({"cursor":"00.1"}),
        json!({"cursor":format!("{}.01","a".repeat(32))}),
    ] {
        let mut args = json!({"owner":"owner","repo":"repo"});
        args.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            call(&client, "list_issues", &args)
                .await
                .structured_content
                .unwrap()["kind"],
            "validation_error"
        );
    }
    assert!(http
        .mock_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn huge_bodies_and_repeated_pr_repositories_are_omitted_but_detail_get_is_complete() {
    let http = TestServer::start().await;
    let big = "large 日本語 body\n".repeat(12_000);
    let pr = json!({"number":1,"title":"Review","state":"open","body":big,"head":{"ref":"feature","sha":"b".repeat(40),"repo":{"name":"repo","full_name":"owner/repo","private":false,"description":big}},"base":{"ref":"main","repo":{"name":"repo","full_name":"owner/repo","private":false,"description":big}}});
    // Two independent observations: default summary and explicit full.
    for page in [1, 2] {
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/pulls"))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(if page == 1 {
                json!([pr])
            } else {
                json!([])
            }))
            .expect(2)
            .mount(&http.mock_server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&pr))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let args = json!({"owner":"owner","repo":"repo"});
    let result = output(call(&client, "list_pull_requests", &args).await);
    let summary = &result["pull_requests"][0];
    assert!(summary.get("body").is_none());
    assert_eq!(summary["head"]["repo"], json!({"full_name":"owner/repo"}));
    assert!(summary["_list"]["omitted_fields"]
        .as_array()
        .unwrap()
        .contains(&json!("/body")));
    let full = call(
        &client,
        "list_pull_requests",
        &json!({"owner":"owner","repo":"repo","view":"full"}),
    )
    .await;
    assert_eq!(
        full.structured_content.unwrap()["kind"],
        "response_too_large"
    );
    let detail = call(
        &client,
        "get_pull_request",
        &json!({"owner":"owner","repo":"repo","pull_number":1}),
    )
    .await;
    assert_eq!(detail.is_error, Some(false));
    let detail = detail.structured_content.unwrap();
    // Detail getters deliberately retain their separate HTTP limits, not the list cap.
    assert_eq!(detail["body"], big);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn comment_preview_is_utf8_safe_explicit_and_full_view_preserves_markdown() {
    let http = TestServer::start().await;
    let text = format!(
        "start\n\n    code\n{}\nhard break  \nend",
        "日本語".repeat(180)
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":1,"body":text}])))
        .expect(2)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let mut args = json!({"owner":"owner","repo":"repo","issue_number":1});
    let summary = output(call(&client, "list_issue_comments", &args).await);
    let preview = summary["comments"][0]["body"].as_str().unwrap();
    assert!(text.starts_with(preview));
    assert!(preview.len() <= 1024);
    assert_eq!(
        summary["comments"][0]["_list"]["truncated_fields"],
        json!(["/body"])
    );
    args["view"] = json!("full");
    let full = output(call(&client, "list_issue_comments", &args).await);
    assert_eq!(full["comments"][0]["body"], text);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn byte_limited_pages_count_both_mcp_representations_and_preserve_all_ids() {
    let http = TestServer::start().await;
    let rows: Vec<_> = (1..=101)
        .map(|i| json!({"id":i,"body":"\"quoted\\text\n日本語".repeat(80)}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&rows))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let (items, pages) = collect(
        &client,
        "list_issue_comments",
        json!({"owner":"owner","repo":"repo","issue_number":1,"view":"full","limit":100}),
        "comments",
    )
    .await;
    assert!(pages.len() > 2);
    assert_eq!(pages[0]["range"]["limited_by"], "bytes");
    for (actual, source) in items.iter().zip(rows) {
        assert_eq!(actual["id"], source["id"]);
        assert_eq!(actual["body"], source["body"]);
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn changed_upstream_is_not_refetched_by_a_continuation_but_a_fresh_call_observes_it() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let http = TestServer::start().await;
    let changed = Arc::new(AtomicBool::new(false));
    let visible = changed.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/labels"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(if visible.load(Ordering::SeqCst) {
                json!([{"name":"new"}])
            } else {
                json!([{"name":"a"},{"name":"b"}])
            })
        })
        .expect(2)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let mut args = json!({"owner":"owner","repo":"repo","limit":1});
    let first = output(call(&client, "list_labels", &args).await);
    changed.store(true, Ordering::SeqCst);
    args["cursor"] = first["next_cursor"].clone();
    let next = output(call(&client, "list_labels", &args).await);
    assert_eq!(next["labels"][0]["name"], "b");
    assert_eq!(next["range"]["consistency"], "cached_observation");
    args.as_object_mut().unwrap().remove("cursor");
    let fresh = output(call(&client, "list_labels", &args).await);
    assert_eq!(fresh["labels"][0]["name"], "new");
    assert_ne!(
        fresh["range"]["observation_id"],
        next["range"]["observation_id"]
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn later_upstream_failure_does_not_become_a_partial_success() {
    let http = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([{"number":1,"title":"Issue","state":"open"}])),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&http.mock_server)
        .await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let error = call(
        &client,
        "list_issues",
        &json!({"owner":"owner","repo":"repo","limit":1}),
    )
    .await;
    let error = error.structured_content.unwrap();
    assert_eq!(error["kind"], "api_error");
    assert!(error.get("next_cursor").is_none());
    assert!(error.get("issues").is_none());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn summaries_mark_nested_array_truncation_and_preserve_unknown_flags_and_identifiers() {
    let http = TestServer::start().await;
    let name = "x".repeat(1500);
    let labels: Vec<_> = (0..21)
        .map(|i| json!({"name":format!("label-{i}"),"color":"aabbcc"}))
        .collect();
    let issue = json!({"number":1,"title":"日".repeat(1000),"state":"open","labels":labels,"assignees":null});
    paged(&http, "/api/v3/repos/owner/repo/issues", &[issue]).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/branches"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{"name":name,"commit":null}])),
        )
        .expect(1)
        .mount(&http.mock_server)
        .await;
    paged(&http,"/api/v3/repos/owner/repo/pulls",&[json!({"number":1,"title":"PR","state":"open","draft":null,"merged":false,"mergeable":null})]).await;
    let client = connect(GitBucketMcpServer::new(http.client())).await;
    let args = json!({"owner":"owner","repo":"repo"});
    let issues = output(call(&client, "list_issues", &args).await);
    let summary = &issues["issues"][0];
    assert_eq!(summary["labels"].as_array().unwrap().len(), 20);
    assert_eq!(
        summary["_list"]["truncated_fields"],
        json!(["/labels", "/title"])
    );
    assert!(summary["title"].as_str().unwrap().len() <= 1024);
    let branches = output(call(&client, "list_branches", &args).await);
    assert_eq!(branches["branches"][0]["name"], name);
    assert_eq!(branches["branches"][0]["commit"], Value::Null);
    let prs = output(call(&client, "list_pull_requests", &args).await);
    assert!(prs["pull_requests"][0]["draft"].is_null());
    assert_eq!(prs["pull_requests"][0]["merged"], false);
    assert!(prs["pull_requests"][0]["mergeable"].is_null());
    client.cancel().await.unwrap();
}
