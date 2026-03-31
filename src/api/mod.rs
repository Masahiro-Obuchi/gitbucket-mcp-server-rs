use std::future::Future;
use std::pin::Pin;

use crate::error::Result;
use crate::models::comment::{Comment, CreateComment};
use crate::models::issue::{CreateIssue, Issue, UpdateIssue};
use crate::models::label::{CreateLabel, Label};
use crate::models::pull_request::{CreatePullRequest, MergePullRequest, MergeResult, PullRequest};
use crate::models::repository::{Branch, CreateRepository, Repository};
use crate::models::user::User;

pub mod client;
pub mod issue;
pub mod label;
pub mod pull_request;
pub mod repository;
pub mod user;
pub mod web;

pub type ApiFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

pub trait GitBucketApi: std::fmt::Debug + Send + Sync {
    fn get_authenticated_user(&self) -> ApiFuture<'_, User>;
    fn get_user<'a>(&'a self, username: &'a str) -> ApiFuture<'a, User>;

    fn list_repositories<'a>(&'a self, owner: &'a str) -> ApiFuture<'a, Vec<Repository>>;
    fn get_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository>;
    fn create_repository<'a>(&'a self, body: &'a CreateRepository) -> ApiFuture<'a, Repository>;
    fn fork_repository<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Repository>;
    fn list_branches<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Branch>>;
    fn list_labels<'a>(&'a self, owner: &'a str, repo: &'a str) -> ApiFuture<'a, Vec<Label>>;
    fn get_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, Label>;
    fn create_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateLabel,
    ) -> ApiFuture<'a, Label>;
    fn delete_label<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        name: &'a str,
    ) -> ApiFuture<'a, ()>;

    fn list_issues<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<Issue>>;
    fn get_issue<'a>(&'a self, owner: &'a str, repo: &'a str, number: u64) -> ApiFuture<'a, Issue>;
    fn create_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreateIssue,
    ) -> ApiFuture<'a, Issue>;
    fn update_issue<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a UpdateIssue,
    ) -> ApiFuture<'a, Issue>;
    fn list_issue_comments<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, Vec<Comment>>;
    fn add_issue_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment>;

    fn list_pull_requests<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        state: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<PullRequest>>;
    fn get_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
    ) -> ApiFuture<'a, PullRequest>;
    fn create_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        body: &'a CreatePullRequest,
    ) -> ApiFuture<'a, PullRequest>;
    fn merge_pull_request<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a MergePullRequest,
    ) -> ApiFuture<'a, MergeResult>;
    fn add_pull_request_comment<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        number: u64,
        body: &'a CreateComment,
    ) -> ApiFuture<'a, Comment>;
}
