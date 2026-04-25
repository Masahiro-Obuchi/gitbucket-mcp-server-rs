use super::client::GitBucketClient;
use super::{ApiFuture, GitBucketApi};
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};
use crate::models::label::{CreateLabel, Label, UpdateLabel};
use crate::models::milestone::{CreateMilestone, Milestone, UpdateMilestone};
use crate::models::pull_request::{
    CreatePullRequest, MergePullRequest, MergeResult, PullRequest, UpdatePullRequest,
};
use crate::models::repository::{Branch, CreateRepository, Repository};
use crate::models::user::User;

impl GitBucketApi for GitBucketClient {
    fn get_authenticated_user(&self) -> ApiFuture<'_, User> {
        Box::pin(async move { GitBucketClient::get_authenticated_user(self).await })
    }

    fn get_user<'a>(&'a self, username: &'a str) -> ApiFuture<'a, User> {
        Box::pin(async move { GitBucketClient::get_user(self, username).await })
    }

    fn list_repositories<'a>(&'a self, owner: &'a str) -> ApiFuture<'a, Vec<Repository>> {
        Box::pin(async move { GitBucketClient::list_repositories(self, owner).await })
    }

    fn get_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::get_repository(self, owner, repo).await })
    }

    fn create_repository<'a>(&'a self, body: &'a CreateRepository) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::create_repository(self, body).await })
    }

    fn fork_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository> {
        Box::pin(async move { GitBucketClient::fork_repository(self, owner, repo).await })
    }

    fn list_branches<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Branch>> {
        Box::pin(async move { GitBucketClient::list_branches(self, owner, repo).await })
    }

    fn list_labels<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Label>> {
        Box::pin(async move { GitBucketClient::list_labels(self, owner, repo).await })
    }

    fn get_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::get_label(self, owner, repo, name).await })
    }

    fn create_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateLabel,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::create_label(self, owner, repo, body).await })
    }

    fn update_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
        body: &'a UpdateLabel,
    ) -> ApiFuture<'a, Label> {
        Box::pin(async move { GitBucketClient::update_label(self, owner, repo, name, body).await })
    }

    fn delete_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move { GitBucketClient::delete_label(self, owner, repo, name).await })
    }

    fn list_milestones<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Milestone>> {
        Box::pin(async move { GitBucketClient::list_milestones(self, owner, repo, state).await })
    }

    fn get_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(async move { GitBucketClient::get_milestone(self, owner, repo, number).await })
    }

    fn create_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(async move { GitBucketClient::create_milestone(self, owner, repo, body).await })
    }

    fn update_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateMilestone,
    ) -> ApiFuture<'a, Milestone> {
        Box::pin(
            async move { GitBucketClient::update_milestone(self, owner, repo, number, body).await },
        )
    }

    fn delete_milestone<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move { GitBucketClient::delete_milestone(self, owner, repo, number).await })
    }

    fn list_issues<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Issue>> {
        Box::pin(async move { GitBucketClient::list_issues(self, owner, repo, state).await })
    }

    fn get_issue<'a>(&'a self, owner: &'a str, repo: &'a str, number: u64) -> ApiFuture<'a, Issue> {
        Box::pin(async move { GitBucketClient::get_issue(self, owner, repo, number).await })
    }

    fn create_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateIssue,
    ) -> ApiFuture<'a, Issue> {
        Box::pin(async move { GitBucketClient::create_issue(self, owner, repo, body).await })
    }

    fn update_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateIssue,
    ) -> ApiFuture<'a, Issue> {
        Box::pin(
            async move { GitBucketClient::update_issue(self, owner, repo, number, body).await },
        )
    }

    fn list_issue_comments<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Vec<Comment>> {
        Box::pin(
            async move { GitBucketClient::list_issue_comments(self, owner, repo, number).await },
        )
    }

    fn add_issue_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        Box::pin(async move {
            GitBucketClient::add_issue_comment(self, owner, repo, number, body).await
        })
    }

    fn list_pull_requests<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<PullRequest>> {
        Box::pin(async move { GitBucketClient::list_pull_requests(self, owner, repo, state).await })
    }

    fn get_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move { GitBucketClient::get_pull_request(self, owner, repo, number).await })
    }

    fn create_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move { GitBucketClient::create_pull_request(self, owner, repo, body).await })
    }

    fn update_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdatePullRequest,
    ) -> ApiFuture<'a, PullRequest> {
        Box::pin(async move {
            GitBucketClient::update_pull_request(self, owner, repo, number, body).await
        })
    }

    fn merge_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a MergePullRequest,
    ) -> ApiFuture<'a, MergeResult> {
        Box::pin(async move {
            GitBucketClient::merge_pull_request(self, owner, repo, number, body).await
        })
    }

    fn add_pull_request_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment> {
        Box::pin(async move {
            GitBucketClient::add_pull_request_comment(self, owner, repo, number, body).await
        })
    }
}
