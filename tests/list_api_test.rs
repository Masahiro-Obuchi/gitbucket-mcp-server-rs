#[allow(dead_code)]
mod common;

use std::collections::BTreeSet;

use common::TestServer;
use gitbucket_mcp_server::api::client::GitBucketClient;
use gitbucket_mcp_server::error::GbMcpError;
use serde_json::{json, Value};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

// Official GitBucket 4.44.0/4.48.0 use PullRequestLimit=25 for these
// searches. Neither endpoint honors a requested per_page=100.
async fn check_numbered_boundaries(pulls: bool) {
    for state in ["open", "closed", "all"] {
        for count in [0usize, 1, 24, 25, 26, 50, 51] {
            let server = TestServer::start().await;
            let endpoint = if pulls { "pulls" } else { "issues" };
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/owner/repo/{endpoint}")))
                .and(query_param("state", state))
                .and(query_param("per_page", "25"))
                .respond_with(move |request: &wiremock::Request| {
                    let page: usize = request
                        .url
                        .query_pairs()
                        .find(|(key, _)| key == "page")
                        .unwrap()
                        .1
                        .parse()
                        .unwrap();
                    let start = ((page - 1) * 25).min(count);
                    let end = (start + 25).min(count);
                    let items: Vec<_> = (start..end)
                        .rev()
                        .map(|i| {
                            json!({
                                "number":i+1,"title":format!("item-{i}"),
                                "state":if state=="closed" {"closed"} else {"open"}
                            })
                        })
                        .collect();
                    ResponseTemplate::new(200).set_body_json(items)
                })
                .expect(count.div_ceil(25) as u64 + 1)
                .mount(&server.mock_server)
                .await;
            let client = server.client();
            let numbers: BTreeSet<_> = if pulls {
                client
                    .list_pull_requests("owner", "repo", Some(state))
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|item| item.number)
                    .collect()
            } else {
                client
                    .list_issues("owner", "repo", Some(state))
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|item| item.number)
                    .collect()
            };
            assert_eq!(
                numbers,
                (1..=count as u64).collect(),
                "{endpoint}/{state}/{count}"
            );
        }
    }
}

#[tokio::test]
async fn test_issue_pages_include_every_number_at_page_boundaries() {
    check_numbered_boundaries(false).await;
}

#[tokio::test]
async fn test_pull_request_pages_include_every_number_at_page_boundaries() {
    check_numbered_boundaries(true).await;
}

#[derive(Clone, Copy, Debug)]
enum Unpaged {
    Repositories,
    Branches,
    Labels,
    Milestones,
    Comments,
}

impl Unpaged {
    fn path(self) -> &'static str {
        match self {
            Self::Repositories => "/api/v3/users/owner/repos",
            Self::Branches => "/api/v3/repos/owner/repo/branches",
            Self::Labels => "/api/v3/repos/owner/repo/labels",
            Self::Milestones => "/api/v3/repos/owner/repo/milestones",
            Self::Comments => "/api/v3/repos/owner/repo/issues/42/comments",
        }
    }

    fn item(self, i: usize) -> Value {
        match self {
            Self::Repositories => {
                json!({"name":i.to_string(),"full_name":format!("owner/{i}"),"private":false})
            }
            Self::Branches | Self::Labels => json!({"name":i.to_string()}),
            Self::Milestones => json!({"number":i,"title":i.to_string(),"state":"closed"}),
            Self::Comments => json!({"id":i,"body":"comment"}),
        }
    }

    async fn list(self, client: &GitBucketClient) -> Vec<String> {
        match self {
            Self::Repositories => client
                .list_repositories("owner")
                .await
                .unwrap()
                .into_iter()
                .map(|v| v.name)
                .collect(),
            Self::Branches => client
                .list_branches("owner", "repo")
                .await
                .unwrap()
                .into_iter()
                .map(|v| v.name)
                .collect(),
            Self::Labels => client
                .list_labels("owner", "repo")
                .await
                .unwrap()
                .into_iter()
                .map(|v| v.name)
                .collect(),
            Self::Milestones => client
                .list_milestones("owner", "repo", Some("closed"))
                .await
                .unwrap()
                .into_iter()
                .map(|v| v.number.to_string())
                .collect(),
            Self::Comments => client
                .list_issue_comments("owner", "repo", 42)
                .await
                .unwrap()
                .into_iter()
                .map(|v| v.id.to_string())
                .collect(),
        }
    }
}

#[tokio::test]
async fn test_unpaged_lists_are_requested_once_even_at_100_items() {
    for domain in [
        Unpaged::Repositories,
        Unpaged::Branches,
        Unpaged::Labels,
        Unpaged::Milestones,
        Unpaged::Comments,
    ] {
        for count in [0, 1, 99, 100, 101] {
            let server = TestServer::start().await;
            Mock::given(method("GET"))
                .and(path(domain.path()))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json((1..=count).map(|i| domain.item(i)).collect::<Vec<_>>()),
                )
                .expect(1)
                .mount(&server.mock_server)
                .await;
            let result = domain.list(&server.client()).await;
            assert_eq!(
                result,
                (1..=count).map(|i| i.to_string()).collect::<Vec<_>>(),
                "{domain:?}/{count}"
            );
            let requests = server.mock_server.received_requests().await.unwrap();
            let query: Vec<_> = requests[0]
                .url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let expected = if matches!(domain, Unpaged::Milestones) {
                vec![("state".into(), "closed".into())]
            } else {
                vec![]
            };
            assert_eq!(query, expected, "{domain:?}");
        }
    }
}

#[tokio::test]
async fn test_org_repository_fallback_does_not_paginate() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/owner/repos"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/owner/repos"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                (1..=101)
                    .map(|i| Unpaged::Repositories.item(i))
                    .collect::<Vec<_>>(),
            ),
        )
        .expect(1)
        .mount(&server.mock_server)
        .await;
    assert_eq!(
        server
            .client()
            .list_repositories("owner")
            .await
            .unwrap()
            .len(),
        101
    );
    for request in server.mock_server.received_requests().await.unwrap() {
        assert_eq!(request.url.query(), None);
    }
}

#[tokio::test]
async fn test_repeated_page_is_rejected_even_if_order_and_content_change() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls"))
        .respond_with(|request: &wiremock::Request| {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
                .parse::<usize>()
                .unwrap();
            let mut items: Vec<_> = (1..=25)
                .map(
                    |number| json!({"number":number,"title":format!("page-{page}"),"state":"open"}),
                )
                .collect();
            if page > 1 {
                items.reverse();
            }
            ResponseTemplate::new(200).set_body_json(items)
        })
        .expect(2)
        .mount(&server.mock_server)
        .await;
    let err = server
        .client()
        .list_pull_requests("owner", "repo", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[tokio::test]
async fn test_overlapping_pages_are_not_returned_as_a_complete_list() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(|request: &wiremock::Request| {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
                .parse::<usize>()
                .unwrap();
            let start = if page == 1 { 1 } else { 25 };
            ResponseTemplate::new(200).set_body_json(
                (start..start + 25)
                    .map(|number| json!({"number":number,"title":"issue","state":"open"}))
                    .collect::<Vec<_>>(),
            )
        })
        .expect(2)
        .mount(&server.mock_server)
        .await;
    assert!(server
        .client()
        .list_issues("owner", "repo", None)
        .await
        .is_err());
}

#[tokio::test]
async fn test_later_page_http_error_does_not_return_partial_results() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(|request: &wiremock::Request| {
            if request
                .url
                .query_pairs()
                .any(|(k, v)| k == "page" && v == "1")
            {
                ResponseTemplate::new(200).set_body_json(
                    (1..=25)
                        .map(|number| json!({"number":number,"title":"issue","state":"open"}))
                        .collect::<Vec<_>>(),
                )
            } else {
                ResponseTemplate::new(503)
            }
        })
        .expect(2)
        .mount(&server.mock_server)
        .await;
    assert!(matches!(
        server.client().list_issues("owner", "repo", None).await,
        Err(GbMcpError::Api { status: 503, .. })
    ));
}

#[tokio::test]
async fn test_pagination_limit_does_not_return_partial_results() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(|request: &wiremock::Request| {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
                .parse::<u64>()
                .unwrap();
            ResponseTemplate::new(200)
                .set_body_json(vec![json!({"number":page,"title":"issue","state":"open"})])
        })
        .expect(100)
        .mount(&server.mock_server)
        .await;
    let err = server
        .client()
        .list_issues("owner", "repo", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Pagination exceeded"), "{err}");
}

#[tokio::test]
async fn test_short_nonfinal_pages_do_not_truncate_results() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(|request: &wiremock::Request| {
            let page = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "page")
                .unwrap()
                .1
                .parse::<u64>()
                .unwrap();
            let items = if page <= 2 {
                vec![json!({"number":page,"title":"issue","state":"open"})]
            } else {
                vec![]
            };
            ResponseTemplate::new(200).set_body_json(items)
        })
        .expect(3)
        .mount(&server.mock_server)
        .await;
    let issues = server
        .client()
        .list_issues("owner", "repo", None)
        .await
        .unwrap();
    assert_eq!(
        issues.into_iter().map(|i| i.number).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[tokio::test]
async fn test_later_page_invalid_json_does_not_return_partial_results() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .respond_with(|request: &wiremock::Request| {
            if request
                .url
                .query_pairs()
                .any(|(k, v)| k == "page" && v == "1")
            {
                ResponseTemplate::new(200)
                    .set_body_json(vec![json!({"number":1,"title":"issue","state":"open"})])
            } else {
                ResponseTemplate::new(200).set_body_string("not json")
            }
        })
        .expect(2)
        .mount(&server.mock_server)
        .await;
    assert!(matches!(
        server.client().list_issues("owner", "repo", None).await,
        Err(GbMcpError::Json(_))
    ));
}

#[tokio::test]
async fn test_default_state_and_context_path_are_preserved() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/gitbucket/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let client = GitBucketClient::new(
        &format!("{}/gitbucket", server.mock_server.uri()),
        "test-token",
    )
    .unwrap();
    assert!(client
        .list_issues("owner", "repo", None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn test_public_pagination_helper_decodes_before_requesting_another_page() {
    let server = TestServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/custom-paged-endpoint"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"invalid_user":true}])))
        .expect(1)
        .mount(&server.mock_server)
        .await;
    let result = server
        .client()
        .get_paginated::<gitbucket_mcp_server::models::user::User>("/custom-paged-endpoint", &[])
        .await;
    assert!(matches!(result, Err(GbMcpError::Json(_))));
}
