use std::sync::{Arc, Mutex};

use crate::api::{ApiFuture, GitBucketApi};
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, Label, UpdateIssue};
use crate::models::label::{CreateLabel, Label as RepositoryLabel, UpdateLabel};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};
use crate::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest, UpdatePullRequest,
};
use crate::models::repository::{Branch, BranchCommit, CreateRepository, Repository};
use crate::models::user::User;

#[derive(Debug, Clone)]
pub enum RecordedCall {
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
    UpdateLabel {
        owner: String,
        repo: String,
        name: String,
        body: UpdateLabel,
    },
    DeleteLabel {
        owner: String,
        repo: String,
        name: String,
    },
    ListMilestones {
        owner: String,
        repo: String,
        state: Option<String>,
    },
    GetMilestone {
        owner: String,
        repo: String,
        number: u64,
    },
    CreateMilestone {
        owner: String,
        repo: String,
        body: CreateMilestone,
    },
    UpdateMilestone {
        owner: String,
        repo: String,
        number: u64,
        body: UpdateMilestone,
    },
    DeleteMilestone {
        owner: String,
        repo: String,
        number: u64,
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
    UpdatePullRequest {
        owner: String,
        repo: String,
        number: u64,
        body: UpdatePullRequest,
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

#[derive(Debug, Clone)]
pub struct MockApi {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    user: User,
    repositories: Vec<Repository>,
    repository: Repository,
    branches: Vec<Branch>,
    labels: Vec<RepositoryLabel>,
    label: RepositoryLabel,
    milestones: Vec<Milestone>,
    milestone: Milestone,
    issues: Vec<Issue>,
    issue: Issue,
    comments: Vec<Comment>,
    comment: Comment,
    pull_requests: Vec<PullRequest>,
    pull_request: PullRequest,
    merge_result: MergeResult,
}

impl Default for MockApi {
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
        let branch = Branch {
            name: "main".to_string(),
            commit: Some(BranchCommit {
                sha: "abc123".to_string(),
            }),
        };
        let label = RepositoryLabel {
            name: "bug".to_string(),
            color: Some("fc2929".to_string()),
            description: Some("Broken behavior".to_string()),
            url: None,
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
        let milestone = Milestone {
            number: 7,
            title: "v1.0".to_string(),
            state: "open".to_string(),
            description: Some("First release".to_string()),
            due_on: Some("2026-04-01T00:00:00Z".to_string()),
            html_url: None,
            url: None,
            creator: Some(user.clone()),
            open_issues: Some(3),
            closed_issues: Some(1),
            created_at: None,
            updated_at: None,
            closed_at: None,
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
            calls: Arc::new(Mutex::new(vec![])),
            user,
            repositories: vec![repository.clone()],
            repository,
            branches: vec![branch],
            labels: vec![label.clone()],
            label,
            milestones: vec![milestone.clone()],
            milestone,
            issues: vec![issue.clone()],
            issue,
            comments: vec![comment.clone()],
            comment,
            pull_requests: vec![pull_request.clone()],
            pull_request,
            merge_result,
        }
    }
}

impl MockApi {
    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: RecordedCall) {
        self.calls.lock().unwrap().push(call);
    }
}

impl GitBucketApi for MockApi {
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
        let repositories = self.repositories.clone();
        Box::pin(async move { Ok(repositories) })
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
        let branches = self.branches.clone();
        Box::pin(async move { Ok(branches) })
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
        let labels = self.labels.clone();
        Box::pin(async move { Ok(labels) })
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

    fn update_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
        body: &'a UpdateLabel,
    ) -> ApiFuture<'a, RepositoryLabel> {
        self.record(RecordedCall::UpdateLabel {
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: name.to_string(),
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

    fn list_milestones<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Milestone>> {
        self.record(RecordedCall::ListMilestones {
            owner: owner.to_string(),
            repo: repo.to_string(),
            state: state.map(str::to_string),
        });
        let milestones = self.milestones.clone();
        Box::pin(async move { Ok(milestones) })
    }

    fn get_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Milestone> {
        self.record(RecordedCall::GetMilestone {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        });
        let milestone = self.milestone.clone();
        Box::pin(async move { Ok(milestone) })
    }

    fn create_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        self.record(RecordedCall::CreateMilestone {
            owner: owner.to_string(),
            repo: repo.to_string(),
            body: body.clone(),
        });
        let milestone = self.milestone.clone();
        Box::pin(async move { Ok(milestone) })
    }

    fn update_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        self.record(RecordedCall::UpdateMilestone {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            body: body.clone(),
        });
        let milestone = self.milestone.clone();
        Box::pin(async move { Ok(milestone) })
    }

    fn delete_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, ()> {
        self.record(RecordedCall::DeleteMilestone {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
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
        let issues = self.issues.clone();
        Box::pin(async move { Ok(issues) })
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
        let comments = self.comments.clone();
        Box::pin(async move { Ok(comments) })
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
        let pull_requests = self.pull_requests.clone();
        Box::pin(async move { Ok(pull_requests) })
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

    fn update_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        self.record(RecordedCall::UpdatePullRequest {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
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
