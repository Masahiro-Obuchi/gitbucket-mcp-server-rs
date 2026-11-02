mod common;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use common::TestServer;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn test_get_authenticated_user() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("Authorization", "token test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "login": "testuser",
            "email": "test@example.com",
            "type": "User",
            "site_admin": false
        })))
        .mount(&server.mock_server)
        .await;

    let user = client.get_authenticated_user().await.unwrap();
    assert_eq!(user.login, "testuser");
    assert_eq!(user.email, Some("test@example.com".to_string()));
}

#[tokio::test]
async fn test_get_user() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/users/alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "login": "alice",
            "type": "User"
        })))
        .mount(&server.mock_server)
        .await;

    let user = client.get_user("alice").await.unwrap();
    assert_eq!(user.login, "alice");
}

#[tokio::test]
async fn test_api_error_401() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server.mock_server)
        .await;

    let result = client.get_authenticated_user().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, message } => {
            assert_eq!(status, 401);
            assert_eq!(message, "Unauthorized");
        }
        _ => panic!("Expected Api error, got {:?}", err),
    }
}

#[tokio::test]
async fn test_api_error_404() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/users/nonexistent"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server.mock_server)
        .await;

    let result = client.get_user("nonexistent").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        gitbucket_mcp_server::error::GbMcpError::Api { status, .. } => {
            assert_eq!(status, 404);
        }
        e => panic!("Expected Api error, got {:?}", e),
    }
}

#[tokio::test]
async fn test_list_repositories() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/users/testuser/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "name": "repo1",
                "full_name": "testuser/repo1",
                "private": false,
                "fork": false
            },
            {
                "name": "repo2",
                "full_name": "testuser/repo2",
                "private": true,
                "fork": false
            }
        ])))
        .mount(&server.mock_server)
        .await;

    let repos = client.list_repositories("testuser").await.unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0].name, "repo1");
    assert_eq!(repos[1].name, "repo2");
    assert!(repos[1].is_private);
}

#[tokio::test]
async fn test_list_repositories_fallback_to_org() {
    let server = TestServer::start().await;
    let client = server.client();

    // User endpoint returns 404
    Mock::given(method("GET"))
        .and(path("/api/v3/users/myorg/repos"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server.mock_server)
        .await;

    // Org endpoint succeeds
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/myorg/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "name": "org-repo",
                "full_name": "myorg/org-repo",
                "private": false,
                "fork": false
            }
        ])))
        .mount(&server.mock_server)
        .await;

    let repos = client.list_repositories("myorg").await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "org-repo");
}

#[tokio::test]
async fn test_list_repositories_returns_all_101_items_without_pagination() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/testuser/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            (1..=101).map(|i|serde_json::json!({
                "name":format!("repo-{i}"),"full_name":format!("testuser/repo-{i}"),"private":false
            })).collect::<Vec<_>>()
        )).expect(1).mount(&server.mock_server).await;
    let repos = server.client().list_repositories("testuser").await.unwrap();
    assert_eq!(repos.len(), 101);
    assert_eq!(repos[100].name, "repo-101");
}

#[tokio::test]
async fn test_get_repository() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "myrepo",
            "full_name": "testuser/myrepo",
            "description": "A great repo",
            "private": false,
            "fork": false,
            "default_branch": "main"
        })))
        .mount(&server.mock_server)
        .await;

    let repo = client.get_repository("testuser", "myrepo").await.unwrap();
    assert_eq!(repo.name, "myrepo");
    assert_eq!(repo.description, Some("A great repo".to_string()));
    assert_eq!(repo.default_branch, Some("main".to_string()));
}

#[tokio::test]
async fn test_create_repository() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/user/repos"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "name": "new-repo",
            "full_name": "testuser/new-repo",
            "private": true,
            "fork": false
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::repository::CreateRepository {
        name: "new-repo".to_string(),
        description: None,
        is_private: Some(true),
        auto_init: None,
    };
    let repo = client.create_repository(&body).await.unwrap();
    assert_eq!(repo.name, "new-repo");
    assert!(repo.is_private);
}

#[tokio::test]
async fn test_list_branches() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/branches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "main", "commit": {"sha": "abc123"}},
            {"name": "develop", "commit": {"sha": "def456"}}
        ])))
        .mount(&server.mock_server)
        .await;

    let branches = client.list_branches("testuser", "myrepo").await.unwrap();
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[0].name, "main");
    assert_eq!(branches[1].name, "develop");
}

#[tokio::test]
async fn test_list_labels() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "name": "bug",
                "color": "fc2929",
                "description": "Broken behavior",
                "url": "http://example.test/api/v3/repos/testuser/myrepo/labels/bug"
            }
        ])))
        .mount(&server.mock_server)
        .await;

    let labels = client.list_labels("testuser", "myrepo").await.unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].name, "bug");
    assert_eq!(labels[0].color.as_deref(), Some("fc2929"));
}

#[tokio::test]
async fn test_get_label_url_encodes_name() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/labels/needs%20review"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "needs review",
            "color": "a1b2c3",
            "description": "Needs extra review"
        })))
        .mount(&server.mock_server)
        .await;

    let label = client
        .get_label("testuser", "myrepo", "needs review")
        .await
        .unwrap();
    assert_eq!(label.name, "needs review");
}

#[tokio::test]
async fn test_create_label() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/testuser/myrepo/labels"))
        .and(body_string_contains("\"name\":\"needs-review\""))
        .and(body_string_contains("\"color\":\"a1b2c3\""))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "name": "needs-review",
            "color": "a1b2c3"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::label::CreateLabel {
        name: "needs-review".to_string(),
        color: "a1b2c3".to_string(),
    };
    let label = client
        .create_label("testuser", "myrepo", &body)
        .await
        .unwrap();
    assert_eq!(label.name, "needs-review");
    assert_eq!(label.color.as_deref(), Some("a1b2c3"));
}

#[tokio::test]
async fn test_update_label_url_encodes_name() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/labels/needs%20review"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "needs review",
            "color": "fc2929"
        })))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/testuser/myrepo/labels/needs%20review"))
        .and(body_string_contains("\"name\":\"needs-review\""))
        .and(body_string_contains("\"color\":\"a1b2c3\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "needs-review",
            "color": "a1b2c3"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::label::UpdateLabel {
        new_name: Some("needs-review".to_string()),
        color: Some("a1b2c3".to_string()),
    };
    let label = client
        .update_label("testuser", "myrepo", "needs review", &body)
        .await
        .unwrap();
    assert_eq!(label.name, "needs-review");
    assert_eq!(label.color.as_deref(), Some("a1b2c3"));
}

#[tokio::test]
async fn test_delete_label_url_encodes_name() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("DELETE"))
        .and(path("/api/v3/repos/testuser/myrepo/labels/needs%20review"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server.mock_server)
        .await;

    client
        .delete_label("testuser", "myrepo", "needs review")
        .await
        .unwrap();
}

#[tokio::test]
async fn test_list_milestones() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones"))
        .and(query_param("state", "all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "number": 7,
                "title": "v1.0",
                "state": "open",
                "description": "First release"
            }
        ])))
        .mount(&server.mock_server)
        .await;

    let milestones = client
        .list_milestones("testuser", "myrepo", Some("all"))
        .await
        .unwrap();
    assert_eq!(milestones.len(), 1);
    assert_eq!(milestones[0].title, "v1.0");
}

#[tokio::test]
async fn test_get_milestone() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 7,
            "title": "v1.0",
            "state": "open",
            "description": "First release"
        })))
        .mount(&server.mock_server)
        .await;

    let milestone = client.get_milestone("testuser", "myrepo", 7).await.unwrap();
    assert_eq!(milestone.number, 7);
    assert_eq!(milestone.title, "v1.0");
}

#[tokio::test]
async fn test_create_milestone() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones"))
        .and(body_string_contains("\"title\":\"v1.0\""))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "number": 7,
            "title": "v1.0",
            "state": "open",
            "description": "First release",
            "due_on": "2026-04-01T00:00:00Z"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::milestone::CreateMilestone {
        title: "v1.0".to_string(),
        description: Some("First release".to_string()),
        due_on: Some("2026-04-01".to_string()),
    };
    let milestone = client
        .create_milestone("testuser", "myrepo", &body)
        .await
        .unwrap();
    assert_eq!(milestone.number, 7);
    assert_eq!(milestone.title, "v1.0");
}

#[tokio::test]
async fn test_update_milestone() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number":7,"title":"v1.0","state":"open"
        })))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones/7"))
        .and(body_string_contains("\"state\":\"closed\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 7,
            "title": "v1.1",
            "state": "closed"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::milestone::UpdateMilestone {
        title: Some("v1.1".to_string()),
        description: None,
        due_on: None,
        state: Some("closed".to_string()),
    };
    let milestone = client
        .update_milestone("testuser", "myrepo", 7, &body)
        .await
        .unwrap();
    assert_eq!(milestone.title, "v1.1");
    assert_eq!(milestone.state, "closed");
}

#[tokio::test]
async fn test_delete_milestone() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("DELETE"))
        .and(path("/api/v3/repos/testuser/myrepo/milestones/7"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server.mock_server)
        .await;

    client
        .delete_milestone("testuser", "myrepo", 7)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_list_issues() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .and(query_param("per_page", "25"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "number": 1,
                "title": "Bug",
                "state": "open",
                "labels": [{"name": "bug"}]
            }
        ])))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server.mock_server)
        .await;

    let issues = client
        .list_issues("owner", "repo", Some("open"))
        .await
        .unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].number, 1);
    assert_eq!(issues[0].title, "Bug");
    assert_eq!(issues[0].labels[0].name, "bug");
}

#[tokio::test]
async fn test_list_issue_comments_returns_all_101_items_without_pagination() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/42/comments"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                (1..=101)
                    .map(|i| serde_json::json!({"id":i,"body":format!("comment-{i}")}))
                    .collect::<Vec<_>>(),
            ),
        )
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let comments = server
        .client()
        .list_issue_comments("owner", "repo", 42)
        .await
        .unwrap();
    assert_eq!(comments.len(), 101);
    assert_eq!(comments[100].id, 101);
}

#[tokio::test]
async fn test_get_issue() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 42,
            "title": "Feature request",
            "body": "Please add this",
            "state": "open"
        })))
        .mount(&server.mock_server)
        .await;

    let issue = client.get_issue("owner", "repo", 42).await.unwrap();
    assert_eq!(issue.number, 42);
    assert_eq!(issue.title, "Feature request");
}

#[tokio::test]
async fn test_get_closed_issue_fills_closed_at_from_updated_at_when_missing() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 42,
            "title": "Closed issue",
            "state": "closed",
            "updated_at": "2024-01-02T03:04:05Z",
            "closed_at": null
        })))
        .mount(&server.mock_server)
        .await;

    let issue = client.get_issue("owner", "repo", 42).await.unwrap();
    assert_eq!(issue.state, "closed");
    assert_eq!(issue.closed_at.as_deref(), Some("2024-01-02T03:04:05Z"));
}

#[tokio::test]
async fn test_create_issue() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "number": 5,
            "title": "New issue",
            "state": "open"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::CreateIssue {
        title: "New issue".to_string(),
        body: None,
        labels: None,
        assignees: None,
    };
    let issue = client.create_issue("owner", "repo", &body).await.unwrap();
    assert_eq!(issue.number, 5);
    assert_eq!(issue.title, "New issue");
}

#[tokio::test]
async fn test_update_issue_close() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 1,
            "title": "Bug",
            "state": "closed",
            "updated_at": "2024-01-02T03:04:05Z"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::UpdateIssue {
        state: Some("closed".to_string()),
        title: None,
        body: None,
    };
    let issue = client
        .update_issue("owner", "repo", 1, &body)
        .await
        .unwrap();
    assert_eq!(issue.state, "closed");
    assert_eq!(issue.closed_at.as_deref(), Some("2024-01-02T03:04:05Z"));
}

#[tokio::test]
async fn test_standard_issue_state_uses_web() {
    let server = TestServer::start().await;
    let client = server.standard_client_with_web_auth("alice", "secret-pass");
    let get_issue_call_count = Arc::new(AtomicUsize::new(0));

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with({
            let get_issue_call_count = Arc::clone(&get_issue_call_count);
            move |_request: &wiremock::Request| {
                let call_index = get_issue_call_count.fetch_add(1, Ordering::SeqCst);
                if call_index == 0 {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "number": 1,
                        "title": "Bug",
                        "body": "original body",
                        "state": "open"
                    }))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "number": 1,
                        "title": "Bug",
                        "body": "original body",
                        "state": "closed"
                    }))
                }
            }
        })
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/signin"))
        .and(body_string_contains("userName=alice"))
        .and(body_string_contains("password=secret-pass"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("location", "/dashboard")
                .insert_header("set-cookie", "JSESSIONID=session123; Path=/"),
        )
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/dashboard"))
        .and(header("cookie", "JSESSIONID=session123"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/owner/repo/issue_comments/state"))
        .and(header("cookie", "JSESSIONID=session123"))
        .and(body_string_contains("issueId=1"))
        .and(body_string_contains("action=close"))
        .respond_with(ResponseTemplate::new(200).set_body_string("updated"))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::UpdateIssue {
        state: Some("closed".to_string()),
        title: None,
        body: None,
    };
    let issue = client
        .update_issue("owner", "repo", 1, &body)
        .await
        .unwrap();

    assert_eq!(issue.state, "closed");
    assert_eq!(issue.title, "Bug");
}

#[tokio::test]
async fn test_update_issue_missing_issue_does_not_fallback_on_404() {
    let server = TestServer::start().await;
    let client = server.standard_client_with_web_auth("alice", "secret-pass");

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::UpdateIssue {
        state: Some("closed".to_string()),
        title: None,
        body: None,
    };
    let err = client
        .update_issue("owner", "repo", 404, &body)
        .await
        .unwrap_err();

    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, .. } => assert_eq!(status, 404),
        other => panic!("expected API 404, got {:?}", other),
    }
}

#[tokio::test]
async fn test_standard_issue_title_body_updates_via_web_session() {
    let server = TestServer::start().await;
    let client = server.standard_client_with_web_auth("alice", "secret-pass");
    let get_issue_call_count = Arc::new(AtomicUsize::new(0));

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with({
            let get_issue_call_count = Arc::clone(&get_issue_call_count);
            move |_request: &wiremock::Request| {
                let call_index = get_issue_call_count.fetch_add(1, Ordering::SeqCst);
                if call_index == 0 {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "number": 1,
                        "title": "Bug",
                        "body": "original body",
                        "state": "open"
                    }))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "number": 1,
                        "title": "New title",
                        "body": "updated body",
                        "state": "closed"
                    }))
                }
            }
        })
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/signin"))
        .and(body_string_contains("userName=alice"))
        .and(body_string_contains("password=secret-pass"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header("location", "/dashboard")
                .insert_header("set-cookie", "JSESSIONID=session123; Path=/"),
        )
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/dashboard"))
        .and(header("cookie", "JSESSIONID=session123"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit_title/1"))
        .and(header("cookie", "JSESSIONID=session123"))
        .and(body_string_contains("title=New+title"))
        .respond_with(ResponseTemplate::new(200).set_body_string("title updated"))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/owner/repo/issues/edit/1"))
        .and(header("cookie", "JSESSIONID=session123"))
        .and(body_string_contains("title=New+title"))
        .and(body_string_contains("content=updated+body"))
        .respond_with(ResponseTemplate::new(200).set_body_string("body updated"))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/owner/repo/issue_comments/state"))
        .and(header("cookie", "JSESSIONID=session123"))
        .and(body_string_contains("issueId=1"))
        .and(body_string_contains("action=close"))
        .respond_with(ResponseTemplate::new(200).set_body_string("updated"))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::UpdateIssue {
        state: Some("closed".to_string()),
        title: Some("New title".to_string()),
        body: Some("updated body".to_string()),
    };
    let issue = client
        .update_issue("owner", "repo", 1, &body)
        .await
        .unwrap();

    assert_eq!(issue.state, "closed");
    assert_eq!(issue.title, "New title");
    assert_eq!(issue.body.as_deref(), Some("updated body"));
}

#[tokio::test]
async fn test_update_issue_state_without_web_credentials_returns_clear_error() {
    let server = TestServer::start().await;
    let client = server.standard_client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 1,
            "title": "Bug",
            "state": "open"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::issue::UpdateIssue {
        state: Some("closed".to_string()),
        title: None,
        body: None,
    };
    let err = client
        .update_issue("owner", "repo", 1, &body)
        .await
        .unwrap_err();

    assert!(err
        .to_string()
        .contains("Set GITBUCKET_USERNAME and GITBUCKET_PASSWORD"));
}

#[tokio::test]
async fn test_add_issue_comment() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/issues/1/comments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 10,
            "body": "Great work!",
            "user": {"login": "reviewer"}
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::comment::CreateComment {
        body: "Great work!".to_string(),
    };
    let comment = client
        .add_issue_comment("owner", "repo", 1, &body)
        .await
        .unwrap();
    assert_eq!(comment.id, 10);
    assert_eq!(comment.body, Some("Great work!".to_string()));
}

#[tokio::test]
async fn test_list_pull_requests() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls"))
        .and(query_param("state", "open"))
        .and(query_param("per_page", "25"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "number": 3,
                "title": "Add feature",
                "state": "open",
                "head": {"ref": "feature", "sha": "abc"},
                "base": {"ref": "main", "sha": "def"}
            }
        ])))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server.mock_server)
        .await;

    let prs = client
        .list_pull_requests("owner", "repo", Some("open"))
        .await
        .unwrap();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 3);
    assert_eq!(prs[0].head.as_ref().unwrap().ref_name, "feature");
}

#[tokio::test]
async fn test_list_pull_requests_paginates_in_25_item_pages() {
    let server = TestServer::start().await;
    for (page, start, end) in [(1, 1, 25), (2, 26, 26), (3, 1, 0)] {
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/owner/repo/pulls"))
            .and(query_param("state","open"))
            .and(query_param("per_page","25"))
            .and(query_param("page",page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                (start..=end).map(|i|serde_json::json!({"number":i,"title":format!("PR {i}"),"state":"open"})).collect::<Vec<_>>()
            )).expect(1).mount(&server.mock_server).await;
    }
    let prs = server
        .client()
        .list_pull_requests("owner", "repo", Some("open"))
        .await
        .unwrap();
    assert_eq!(prs.len(), 26);
    assert_eq!(prs[25].number, 26);
}

#[tokio::test]
async fn test_create_pull_request() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/pulls"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "number": 7,
            "title": "New PR",
            "state": "open",
            "head": {"ref": "feature", "sha": "abc"},
            "base": {"ref": "main", "sha": "def"}
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::CreatePullRequest {
        title: "New PR".to_string(),
        head: "feature".to_string(),
        base: "main".to_string(),
        body: None,
    };
    let pr = client
        .create_pull_request("owner", "repo", &body)
        .await
        .unwrap();
    assert_eq!(pr.number, 7);
    assert_eq!(pr.title, "New PR");
}

#[tokio::test]
async fn test_get_pull_request_maps_wrapped_404_to_api_error() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/9999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 404,
            "body": ""
        })))
        .mount(&server.mock_server)
        .await;

    let err = client
        .get_pull_request("owner", "repo", 9999)
        .await
        .unwrap_err();

    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, message } => {
            assert_eq!(status, 404);
            assert_eq!(message, "Not Found");
        }
        err => panic!("expected API 404, got {err:?}"),
    }
}

#[tokio::test]
async fn test_update_pull_request() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/branches/develop"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"name":"develop"})),
        )
        .mount(&server.mock_server)
        .await;

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .and(body_string_contains("\"title\":\"Updated PR\""))
        .and(body_string_contains("\"body\":\"Updated body\""))
        .and(body_string_contains("\"state\":\"closed\""))
        .and(body_string_contains("\"base\":\"develop\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 7,
            "title": "Updated PR",
            "body": "Updated body",
            "state": "closed",
            "base": {"ref": "develop", "sha": "abc"}
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::UpdatePullRequest {
        state: Some("closed".to_string()),
        title: Some("Updated PR".to_string()),
        body: Some("Updated body".to_string()),
        base: Some("develop".to_string()),
    };
    let pr = client
        .update_pull_request("owner", "repo", 7, &body)
        .await
        .unwrap();
    assert_eq!(pr.title, "Updated PR");
    assert_eq!(pr.state, "closed");
    assert_eq!(pr.base.as_ref().unwrap().ref_name, "develop");
}

// GitBucket 4.44.0/4.48.0 PullRequestService.updatePullRequestsByApi updates
// content only inside title.foreach, and replaces body rather than merging it.
// Model that contract rather than returning the JSON we hope to receive.
async fn mount_pull_request_update_contract(
    server: &TestServer,
    initial_body: Option<&str>,
) -> Arc<std::sync::Mutex<serde_json::Value>> {
    let current = Arc::new(std::sync::Mutex::new(serde_json::json!({
        "number": 7, "title": "Original PR", "body": initial_body, "state": "open",
        "head": {"ref": "feature", "sha": "original-head"},
        "base": {"ref": "main", "sha": "base-sha"}
    })));
    let read = Arc::clone(&current);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(read.lock().unwrap().clone())
        })
        .mount(&server.mock_server)
        .await;
    let write = Arc::clone(&current);
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(move |request: &wiremock::Request| {
            let update: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let mut pr = write.lock().unwrap();
            if let Some(title) = update.get("title") {
                pr["title"] = title.clone();
                pr["body"] = update.get("body").cloned().unwrap_or_default();
            }
            if let Some(state) = update.get("state") {
                pr["state"] = state.clone();
            }
            ResponseTemplate::new(200).set_body_json(pr.clone())
        })
        .expect(1)
        .mount(&server.mock_server)
        .await;
    current
}

fn pull_request_update() -> gitbucket_mcp_server::models::pull_request::UpdatePullRequest {
    gitbucket_mcp_server::models::pull_request::UpdatePullRequest {
        state: None,
        title: None,
        body: None,
        base: None,
    }
}

#[tokio::test]
async fn test_update_pull_request_title_preserves_existing_body() {
    let server = TestServer::start().await;
    let original = "    indented code\n\nKeep trailing spaces  \n";
    let current = mount_pull_request_update_contract(&server, Some(original)).await;
    let mut update = pull_request_update();
    update.title = Some("New title".into());
    let pr = server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(pr.title, "New title");
    assert_eq!(pr.body.as_deref(), Some(original));
    assert_eq!(current.lock().unwrap()["body"], original);
}

#[tokio::test]
async fn test_update_pull_request_body_only_preserves_title_and_markdown() {
    let server = TestServer::start().await;
    mount_pull_request_update_contract(&server, Some("old body")).await;
    let body = "    code block\n\nA hard break  \n";
    let mut update = pull_request_update();
    update.body = Some(body.into());
    let pr = server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(pr.title, "Original PR");
    assert_eq!(pr.body.as_deref(), Some(body));
}

#[tokio::test]
async fn test_update_pull_request_can_clear_body_without_changing_title() {
    let server = TestServer::start().await;
    mount_pull_request_update_contract(&server, Some("old body")).await;
    let mut update = pull_request_update();
    update.body = Some(String::new());
    let pr = server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(pr.title, "Original PR");
    assert_eq!(pr.body.as_deref(), Some(""));
}

#[tokio::test]
async fn test_update_pull_request_title_preserves_absent_body() {
    let server = TestServer::start().await;
    mount_pull_request_update_contract(&server, None).await;
    let mut update = pull_request_update();
    update.title = Some("New title".into());
    let pr = server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(pr.title, "New title");
    assert_eq!(pr.body, None);
}

#[tokio::test]
async fn test_update_pull_request_state_does_not_resend_content() {
    let server = TestServer::start().await;
    mount_pull_request_update_contract(&server, Some("keep body")).await;
    let mut update = pull_request_update();
    update.state = Some("closed".into());
    let pr = server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .unwrap();
    assert_eq!(pr.state, "closed");
    assert_eq!(pr.body.as_deref(), Some("keep body"));
    let requests = server.mock_server.received_requests().await.unwrap();
    let request = requests.iter().find(|r| r.method == "PATCH").unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(sent, serde_json::json!({"state":"closed"}));
}

#[tokio::test]
async fn test_update_pull_request_rejects_ignored_success_response() {
    let server = TestServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number":7,"title":"Original PR","state":"open"
        })))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.state = Some("closed".into());
    assert!(server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .is_err());
    // A 200 response that did not apply the request must not trigger another write.
    assert_eq!(
        server.mock_server.received_requests().await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn test_update_pull_request_invalid_base_does_not_patch() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/branches/missing"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.base = Some("missing".into());
    assert!(server
        .client()
        .update_pull_request("owner", "repo", 7, &update)
        .await
        .is_err());
}

#[tokio::test]
async fn test_update_pull_request_missing_content_read_prevents_write() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.title = Some("new title".into());
    assert!(matches!(
        server
            .client()
            .update_pull_request("owner", "repo", 7, &update)
            .await,
        Err(gitbucket_mcp_server::error::GbMcpError::Api { status: 403, .. })
    ));
}

#[tokio::test]
async fn test_update_pull_request_invalid_write_response_is_unknown_without_retry() {
    let server = TestServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.state = Some("closed".into());
    assert!(matches!(
        server
            .client()
            .update_pull_request("owner", "repo", 7, &update)
            .await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationOutcomeUnknown { .. })
    ));
    assert_eq!(
        server.mock_server.received_requests().await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn test_update_pull_request_partial_application_is_not_success() {
    let server = TestServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number":7,"title":"New title","body":"New body","state":"open"
        })))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.title = Some("New title".into());
    update.body = Some("New body".into());
    update.state = Some("closed".into());
    assert!(
        matches!(server.client().update_pull_request("owner", "repo", 7, &update).await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationNotVerified {fields, ..}) if fields == ["state"])
    );
}

#[tokio::test]
async fn test_update_pull_request_existing_base_ignored_by_api_is_not_success() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/branches/develop"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"name":"develop"})),
        )
        .mount(&server.mock_server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number":7,"title":"PR","state":"open","base":{"ref":"main"}
        })))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let mut update = pull_request_update();
    update.base = Some("develop".into());
    assert!(
        matches!(server.client().update_pull_request("owner", "repo", 7, &update).await,
        Err(gitbucket_mcp_server::error::GbMcpError::OperationNotVerified {fields, ..}) if fields == ["base"])
    );
}

#[tokio::test]
async fn test_update_pull_request_write_error_classification_without_retry() {
    use gitbucket_mcp_server::error::GbMcpError;
    for status in [403, 408, 422, 500] {
        let server = TestServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/api/v3/repos/owner/repo/pulls/7"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server.mock_server)
            .await;
        let mut update = pull_request_update();
        update.state = Some("closed".into());
        let error = server
            .client()
            .update_pull_request("owner", "repo", 7, &update)
            .await
            .unwrap_err();
        if status == 408 || status == 500 {
            assert!(
                matches!(error,GbMcpError::OperationOutcomeUnknown {cause,..} if matches!(*cause,GbMcpError::Api{status:s,..} if s==status))
            );
        } else {
            assert!(matches!(error,GbMcpError::Api {status:s,..} if s==status));
        }
        assert_eq!(
            server.mock_server.received_requests().await.unwrap().len(),
            1
        );
    }
}

#[tokio::test]
async fn test_update_pull_request_maps_wrapped_404_to_api_error() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/9999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 404,
            "body": ""
        })))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/9999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 404,
            "body": ""
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::UpdatePullRequest {
        state: Some("closed".to_string()),
        title: None,
        body: None,
        base: None,
    };
    let err = client
        .update_pull_request("owner", "repo", 9999, &body)
        .await
        .unwrap_err();

    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, message } => {
            assert_eq!(status, 404);
            assert_eq!(message, "Not Found");
        }
        err => panic!("expected API 404, got {err:?}"),
    }
}

#[tokio::test]
async fn test_update_pull_request_missing_pr_does_not_fallback_on_404() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("PATCH"))
        .and(path("/api/v3/repos/owner/repo/pulls/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Pull API not found"))
        .mount(&server.mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Pull not found"))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::UpdatePullRequest {
        state: Some("closed".to_string()),
        title: None,
        body: None,
        base: None,
    };
    let err = client
        .update_pull_request("owner", "repo", 404, &body)
        .await
        .unwrap_err();

    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, message } => {
            assert_eq!(status, 404);
            assert_eq!(message, "Not Found");
        }
        err => panic!("expected original API 404, got {err:?}"),
    }
}

#[tokio::test]
async fn test_merge_pull_request() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/owner/repo/pulls/3/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": "merged123",
            "merged": true,
            "message": "Pull Request successfully merged"
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::MergePullRequest {
        commit_message: Some("Merge PR".to_string()),
        sha: None,
        merge_method: None,
    };
    let result = client
        .merge_pull_request("owner", "repo", 3, &body)
        .await
        .unwrap();
    assert_eq!(result.merged, Some(true));
}

#[tokio::test]
async fn test_merge_pull_request_maps_wrapped_404_to_api_error() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("PUT"))
        .and(path("/api/v3/repos/owner/repo/pulls/9999/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": 404,
            "body": ""
        })))
        .mount(&server.mock_server)
        .await;

    let body = gitbucket_mcp_server::models::pull_request::MergePullRequest {
        commit_message: Some("Merge PR".to_string()),
        sha: None,
        merge_method: None,
    };
    let err = client
        .merge_pull_request("owner", "repo", 9999, &body)
        .await
        .unwrap_err();

    match err {
        gitbucket_mcp_server::error::GbMcpError::Api { status, message } => {
            assert_eq!(status, 404);
            assert_eq!(message, "Not Found");
        }
        err => panic!("expected API 404, got {err:?}"),
    }
}

#[tokio::test]
async fn test_fork_repository() {
    let server = TestServer::start().await;
    let client = server.client();

    Mock::given(method("POST"))
        .and(path("/api/v3/repos/owner/repo/forks"))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "name": "repo",
            "full_name": "testuser/repo",
            "private": false,
            "fork": true
        })))
        .mount(&server.mock_server)
        .await;

    let repo = client.fork_repository("owner", "repo").await.unwrap();
    assert_eq!(repo.full_name, "testuser/repo");
    assert!(repo.fork);
}
