use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use gitbucket_mcp_server::api::{ApiFuture, GitBucketApi};
use gitbucket_mcp_server::models::comment::{Comment, CreateComment};
use gitbucket_mcp_server::models::issue::{CreateIssue, Issue, Label, UpdateIssue};
use gitbucket_mcp_server::models::label::{CreateLabel, Label as RepositoryLabel};
use gitbucket_mcp_server::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest,
};
use gitbucket_mcp_server::models::repository::{
    Branch, BranchCommit, CreateRepository, Repository,
};
use gitbucket_mcp_server::models::user::User;
use gitbucket_mcp_server::server::GitBucketMcpServer;
use gitbucket_mcp_server::tools::response::ToolErrorPayload;
use rmcp::model::CallToolRequestParams;
use rmcp::ServiceExt;
use serde_json::{json, Value};

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum RecordedCall {
    GetAuthenticatedUser,
    GetUser {
        username: String,
    },
    ListRepositories {
        owner: String,
    },
    GetRepository {
        owner: String,
        repo: String,
    },
    CreateRepository {
        body: CreateRepository,
    },
    ForkRepository {
        owner: String,
        repo: String,
    },
    ListBranches {
        owner: String,
        repo: String,
    },
    ListLabels {
        owner: String,
        repo: String,
    },
    GetLabel {
        owner: String,
        repo: String,
        name: String,
    },
    CreateLabel {
        owner: String,
        repo: String,
        body: CreateLabel,
    },
    DeleteLabel {
        owner: String,
        repo: String,
        name: String,
    },
    ListIssues {
        owner: String,
        repo: String,
        state: Option<String>,
    },
    GetIssue {
        owner: String,
        repo: String,
        number: u64,
    },
    CreateIssue {
        owner: String,
        repo: String,
        body: CreateIssue,
    },
    UpdateIssue {
        owner: String,
        repo: String,
        number: u64,
        body: UpdateIssue,
    },
    ListIssueComments {
        owner: String,
        repo: String,
        number: u64,
    },
    AddIssueComment {
        owner: String,
        repo: String,
        number: u64,
        body: CreateComment,
    },
    ListPullRequests {
        owner: String,
        repo: String,
        state: Option<String>,
    },
    GetPullRequest {
        owner: String,
        repo: String,
        number: u64,
    },
    CreatePullRequest {
        owner: String,
        repo: String,
        body: CreatePullRequest,
    },
    MergePullRequest {
        owner: String,
        repo: String,
        number: u64,
        body: MergePullRequest,
    },
    AddPullRequestComment {
        owner: String,
        repo: String,
        number: u64,
        body: CreateComment,
    },
}

#[derive(Debug)]
struct IntegrationMockApi {
    calls: Mutex<Vec<RecordedCall>>,
    user: User,
    repository: Repository,
    label: RepositoryLabel,
    issue: Issue,
    comment: Comment,
    pull_request: PullRequest,
    merge_result: MergeResult,
}

impl Default for IntegrationMockApi {
    fn default() -> Self {
        let user = User {
            login: "mock-user".to_string(),
            email: Some("mock@example.com".to_string()),
            user_type: Some("User".to_string()),
            site_admin: Some(false),
            created_at: None,
            avatar_url: None,
            url: None,
            html_url: None,
        };
        let repository = Repository {
            name: "mock-repo".to_string(),
            full_name: "mock-user/mock-repo".to_string(),
            description: Some("Mock repository".to_string()),
            html_url: None,
            clone_url: None,
            is_private: false,
            fork: false,
            default_branch: Some("main".to_string()),
            owner: Some(user.clone()),
            watchers_count: None,
            forks_count: None,
            open_issues_count: None,
        };
        let issue = Issue {
            number: 42,
            title: "Mock issue".to_string(),
            body: Some("Issue body".to_string()),
            state: "open".to_string(),
            user: Some(user.clone()),
            labels: vec![Label {
                name: "bug".to_string(),
                color: None,
                url: None,
            }],
            assignees: vec![],
            html_url: None,
            created_at: None,
            updated_at: None,
            closed_at: None,
            comments: Some(1),
        };
        let label = RepositoryLabel {
            name: "bug".to_string(),
            color: Some("fc2929".to_string()),
            description: Some("Broken behavior".to_string()),
            url: None,
        };
        let comment = Comment {
            id: 1,
            body: Some("Mock comment".to_string()),
            user: Some(user.clone()),
            created_at: None,
            updated_at: None,
            html_url: None,
        };
        let pull_request = PullRequest {
            number: 7,
            title: "Mock PR".to_string(),
            body: Some("PR body".to_string()),
            state: "open".to_string(),
            user: Some(user.clone()),
            html_url: None,
            head: None,
            base: None,
            merged: Some(false),
            mergeable: Some(true),
            created_at: None,
            updated_at: None,
            closed_at: None,
            merged_at: None,
        };
        let merge_result = MergeResult {
            sha: Some("merged-sha".to_string()),
            merged: Some(true),
            message: Some("Pull Request successfully merged".to_string()),
        };

        Self {
            calls: Mutex::new(vec![]),
            user,
            repository,
            label,
            issue,
            comment,
            pull_request,
            merge_result,
        }
    }
}

impl IntegrationMockApi {
    fn record(&self, call: RecordedCall) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }

    fn branch(&self) -> Branch {
        Branch {
            name: "main".to_string(),
            commit: Some(BranchCommit {
                sha: "abc123".to_string(),
            }),
        }
    }
}

impl GitBucketApi for IntegrationMockApi {
    fn get_authenticated_user(&self) -> ApiFuture<'_, User> {
        self.record(RecordedCall::GetAuthenticatedUser);
        let user = self.user.clone();
        Box::pin(async move { Ok(user) })
    }

    fn get_user<'a>(&'a self, username: &'a str) -> ApiFuture<'a, User> {
        self.record(RecordedCall::GetUser {
            username: username.to_string(),
        });
        let user = self.user.clone();
        Box::pin(async move { Ok(user) })
    }

    fn list_repositories<'a>(&'a self, owner: &'a str) -> ApiFuture<'a, Vec<Repository>> {
        self.record(RecordedCall::ListRepositories {
            owner: owner.to_string(),
        });
        let repository = self.repository.clone();
        Box::pin(async move { Ok(vec![repository]) })
    }

    fn get_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        self.record(RecordedCall::GetRepository {
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
        let repository = self.repository.clone();
        Box::pin(async move { Ok(repository) })
    }

    fn create_repository<'a>(&'a self, body: &'a CreateRepository) -> ApiFuture<'a, Repository> {
        self.record(RecordedCall::CreateRepository { body: body.clone() });
        let repository = self.repository.clone();
        Box::pin(async move { Ok(repository) })
    }

    fn fork_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        self.record(RecordedCall::ForkRepository {
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
        let repository = self.repository.clone();
        Box::pin(async move { Ok(repository) })
    }

    fn list_branches<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Branch>> {
        self.record(RecordedCall::ListBranches {
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
        let branch = self.branch();
        Box::pin(async move { Ok(vec![branch]) })
    }

    fn list_labels<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
    ) -> ApiFuture<'a, Vec<RepositoryLabel>> {
        self.record(RecordedCall::ListLabels {
            owner: owner.to_string(),
            repo: repo.to_string(),
        });
        let label = self.label.clone();
        Box::pin(async move { Ok(vec![label]) })
    }

    fn get_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, RepositoryLabel> {
        self.record(RecordedCall::GetLabel {
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: name.to_string(),
        });
        let label = self.label.clone();
        Box::pin(async move { Ok(label) })
    }

    fn create_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateLabel,
    ) -> ApiFuture<'a, RepositoryLabel> {
        self.record(RecordedCall::CreateLabel {
            owner: owner.to_string(),
            repo: repo.to_string(),
            body: body.clone(),
        });
        let label = self.label.clone();
        Box::pin(async move { Ok(label) })
    }

    fn delete_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, ()> {
        self.record(RecordedCall::DeleteLabel {
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: name.to_string(),
        });
        Box::pin(async move { Ok(()) })
    }

    fn list_issues<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Issue>> {
        self.record(RecordedCall::ListIssues {
            owner: owner.to_string(),
            repo: repo.to_string(),
            state: state.map(str::to_string),
        });
        let issue = self.issue.clone();
        Box::pin(async move { Ok(vec![issue]) })
    }

    fn get_issue<'a>(&'a self, owner: &'a str, repo: &'a str, number: u64) -> ApiFuture<'a, Issue> {
        self.record(RecordedCall::GetIssue {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        });
        let issue = self.issue.clone();
        Box::pin(async move { Ok(issue) })
    }

    fn create_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateIssue,
    ) -> ApiFuture<'a, Issue> {
        self.record(RecordedCall::CreateIssue {
            owner: owner.to_string(),
            repo: repo.to_string(),
            body: body.clone(),
        });
        let issue = self.issue.clone();
        Box::pin(async move { Ok(issue) })
    }

    fn update_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateIssue,
    ) -> ApiFuture<'a, Issue> {
        self.record(RecordedCall::UpdateIssue {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            body: body.clone(),
        });
        let issue = self.issue.clone();
        Box::pin(async move { Ok(issue) })
    }

    fn list_issue_comments<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Vec<Comment>> {
        self.record(RecordedCall::ListIssueComments {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        });
        let comment = self.comment.clone();
        Box::pin(async move { Ok(vec![comment]) })
    }

    fn add_issue_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        self.record(RecordedCall::AddIssueComment {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            body: body.clone(),
        });
        let comment = self.comment.clone();
        Box::pin(async move { Ok(comment) })
    }

    fn list_pull_requests<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<PullRequest>> {
        self.record(RecordedCall::ListPullRequests {
            owner: owner.to_string(),
            repo: repo.to_string(),
            state: state.map(str::to_string),
        });
        let pull_request = self.pull_request.clone();
        Box::pin(async move { Ok(vec![pull_request]) })
    }

    fn get_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, PullRequest> {
        self.record(RecordedCall::GetPullRequest {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        });
        let pull_request = self.pull_request.clone();
        Box::pin(async move { Ok(pull_request) })
    }

    fn create_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        self.record(RecordedCall::CreatePullRequest {
            owner: owner.to_string(),
            repo: repo.to_string(),
            body: body.clone(),
        });
        let pull_request = self.pull_request.clone();
        Box::pin(async move { Ok(pull_request) })
    }

    fn merge_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a MergePullRequest,
    ) -> ApiFuture<'a, MergeResult> {
        self.record(RecordedCall::MergePullRequest {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            body: body.clone(),
        });
        let merge_result = self.merge_result.clone();
        Box::pin(async move { Ok(merge_result) })
    }

    fn add_pull_request_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        self.record(RecordedCall::AddPullRequestComment {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            body: body.clone(),
        });
        let comment = self.comment.clone();
        Box::pin(async move { Ok(comment) })
    }
}

async fn spawn_client_and_server() -> (
    rmcp::service::RunningService<rmcp::RoleClient, ()>,
    Arc<IntegrationMockApi>,
) {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let api = Arc::new(IntegrationMockApi::default());
    let server_api = api.clone();

    tokio::spawn(async move {
        let server = GitBucketMcpServer::with_api(server_api)
            .serve(server_transport)
            .await
            .unwrap();
        server.waiting().await.unwrap();
    });

    let client = ().serve(client_transport).await.unwrap();
    (client, api)
}

fn structured_json(result: &rmcp::model::CallToolResult) -> &Value {
    assert_eq!(result.is_error, Some(false));
    result
        .structured_content
        .as_ref()
        .expect("expected structured content")
}

fn structured_error(result: &rmcp::model::CallToolResult) -> ToolErrorPayload {
    assert_eq!(result.is_error, Some(true));
    serde_json::from_value(
        result
            .structured_content
            .clone()
            .expect("expected structured error content"),
    )
    .expect("structured error payload should deserialize")
}

fn required_fields(tools: &[rmcp::model::Tool], tool_name: &str) -> serde_json::Value {
    tools
        .iter()
        .find(|tool| tool.name == tool_name)
        .expect("tool should be listed")
        .input_schema
        .get("required")
        .cloned()
        .expect("required schema array")
}

#[tokio::test]
async fn test_mcp_lists_all_expected_tools_and_required_inputs() {
    let (client, _) = spawn_client_and_server().await;

    let tools = client.list_all_tools().await.unwrap();
    let tool_names: BTreeSet<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();

    let expected = BTreeSet::from([
        "add_issue_comment",
        "add_pull_request_comment",
        "create_issue",
        "create_label",
        "create_pull_request",
        "create_repository",
        "delete_label",
        "fork_repository",
        "get_authenticated_user",
        "get_issue",
        "get_label",
        "get_pull_request",
        "get_repository",
        "get_user",
        "list_branches",
        "list_issue_comments",
        "list_issues",
        "list_labels",
        "list_pull_requests",
        "list_repositories",
        "merge_pull_request",
        "update_issue",
    ]);

    assert_eq!(tool_names, expected);
    assert_eq!(required_fields(&tools, "get_user"), json!(["username"]));
    assert_eq!(
        required_fields(&tools, "list_repositories"),
        json!(["owner"])
    );
    assert_eq!(
        required_fields(&tools, "get_label"),
        json!(["owner", "repo", "name"])
    );
    assert_eq!(
        required_fields(&tools, "create_label"),
        json!(["owner", "repo", "name", "color"])
    );
    assert_eq!(
        required_fields(&tools, "delete_label"),
        json!(["owner", "repo", "name"])
    );
    assert_eq!(
        required_fields(&tools, "create_issue"),
        json!(["owner", "repo", "title"])
    );
    assert_eq!(
        required_fields(&tools, "merge_pull_request"),
        json!(["owner", "repo", "pull_number"])
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_get_authenticated_user_returns_json_and_hits_api() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_authenticated_user")
                .with_arguments(json!({}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_json(&result)["login"].as_str(),
        Some("mock-user")
    );
    match api.calls().as_slice() {
        [RecordedCall::GetAuthenticatedUser] => {}
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_list_repositories_trims_owner_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_repositories").with_arguments(
                json!({
                    "owner": "  mock-user  "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_json(&result)[0]["full_name"].as_str(),
        Some("mock-user/mock-repo")
    );
    match api.calls().as_slice() {
        [RecordedCall::ListRepositories { owner }] => assert_eq!(owner, "mock-user"),
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_get_repository_trims_fields_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_repository").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(structured_json(&result)["name"].as_str(), Some("mock-repo"));
    match api.calls().as_slice() {
        [RecordedCall::GetRepository { owner, repo }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_list_labels_trims_fields_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_labels").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(structured_json(&result)[0]["name"].as_str(), Some("bug"));
    match api.calls().as_slice() {
        [RecordedCall::ListLabels { owner, repo }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_create_label_normalizes_color_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("create_label").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo ",
                    "name": "  needs-review  ",
                    "color": "  #A1B2C3  ",
                    "description": "  Needs extra review  "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(structured_json(&result)["name"].as_str(), Some("bug"));
    match api.calls().as_slice() {
        [RecordedCall::CreateLabel { owner, repo, body }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(body.name, "needs-review");
            assert_eq!(body.color, "a1b2c3");
            assert_eq!(body.description.as_deref(), Some("Needs extra review"));
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_delete_label_trims_fields_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("delete_label").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo ",
                    "name": "  bug  "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(structured_json(&result)["deleted"].as_bool(), Some(true));
    assert_eq!(structured_json(&result)["name"].as_str(), Some("bug"));
    match api.calls().as_slice() {
        [RecordedCall::DeleteLabel { owner, repo, name }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(name, "bug");
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_create_issue_trims_fields_and_hits_api() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("create_issue").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo ",
                    "title": "  New issue  ",
                    "body": "  body text  ",
                    "labels": ["bug"],
                    "assignees": ["alice"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_json(&result)["title"].as_str(),
        Some("Mock issue")
    );
    match api.calls().as_slice() {
        [RecordedCall::CreateIssue { owner, repo, body }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(body.title, "New issue");
            assert_eq!(body.body.as_deref(), Some("body text"));
            assert_eq!(body.labels.as_deref(), Some(&["bug".to_string()][..]));
            assert_eq!(body.assignees.as_deref(), Some(&["alice".to_string()][..]));
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_list_pull_requests_passes_state_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_pull_requests").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo ",
                    "state": "closed"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_json(&result)[0]["title"].as_str(),
        Some("Mock PR")
    );
    match api.calls().as_slice() {
        [RecordedCall::ListPullRequests { owner, repo, state }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(state.as_deref(), Some("closed"));
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_merge_pull_request_trims_commit_message_and_serializes_json() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("merge_pull_request").with_arguments(
                json!({
                    "owner": " owner ",
                    "repo": " repo ",
                    "pull_number": 7,
                    "commit_message": "  merge message  "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(structured_json(&result)["merged"].as_bool(), Some(true));
    match api.calls().as_slice() {
        [RecordedCall::MergePullRequest {
            owner,
            repo,
            number,
            body,
        }] => {
            assert_eq!(owner, "owner");
            assert_eq!(repo, "repo");
            assert_eq!(*number, 7);
            assert_eq!(body.commit_message.as_deref(), Some("merge message"));
            assert_eq!(body.sha, None);
            assert_eq!(body.merge_method, None);
        }
        calls => panic!("unexpected calls: {calls:?}"),
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_returns_validation_error_for_blank_username() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("get_user").with_arguments(
                json!({
                    "username": "   "
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_error(&result),
        ToolErrorPayload {
            kind: "validation_error".to_string(),
            message: "username must not be empty".to_string(),
            status: None,
        }
    );
    assert!(api.calls().is_empty());

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_rejects_invalid_label_color_before_api_call() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("create_label").with_arguments(
                json!({
                    "owner": "owner",
                    "repo": "repo",
                    "name": "bug",
                    "color": "zzz"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_error(&result),
        ToolErrorPayload {
            kind: "validation_error".to_string(),
            message: "color must be a 6-digit hex value like ff0000".to_string(),
            status: None,
        }
    );
    assert!(api.calls().is_empty());

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_returns_validation_error_for_empty_issue_update() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("update_issue").with_arguments(
                json!({
                    "owner": "owner",
                    "repo": "repo",
                    "issue_number": 42
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_error(&result),
        ToolErrorPayload {
            kind: "validation_error".to_string(),
            message: "at least one of state, title, or body must be provided".to_string(),
            status: None,
        }
    );
    assert!(api.calls().is_empty());

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn test_mcp_call_tool_rejects_invalid_state_before_api_call() {
    let (client, api) = spawn_client_and_server().await;

    let result = client
        .call_tool(
            CallToolRequestParams::new("list_pull_requests").with_arguments(
                json!({
                    "owner": "owner",
                    "repo": "repo",
                    "state": "merged"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(
        structured_error(&result),
        ToolErrorPayload {
            kind: "validation_error".to_string(),
            message: "state must be one of: open, closed, all".to_string(),
            status: None,
        }
    );
    assert!(api.calls().is_empty());

    client.cancel().await.unwrap();
}
