//! Pull request update orchestration, separate from HTTP request construction.

use super::{client::GitBucketClient, outcome::Operation};
use crate::error::{GbMcpError, Result};
use crate::models::operation::{OperationTarget, ResourceKind};
use crate::models::pull_request::{
    GitBucketPullRequestContent, GitBucketPullRequestUpdate, PullRequest, UpdatePullRequest,
};
use crate::models::repository::Branch;

pub(super) async fn update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    change: &UpdatePullRequest,
) -> Result<PullRequest> {
    // Only content changes need a read: state/base updates must not overwrite
    // content with values fetched before somebody else's edit.
    let current = if (change.title.is_some() || change.body.is_some())
        && (change.title.is_none() || change.body.is_none())
    {
        Some(client.get_pull_request(owner, repo, number).await?)
    } else {
        None
    };
    if current.as_ref().is_some_and(|pr| pr.number != number) {
        return Err(GbMcpError::Other(
            "GitBucket returned a different pull request; no write was sent".into(),
        ));
    }
    if let Some(base) = &change.base {
        validate_base(client, owner, repo, base).await?;
    }

    let content = if change.title.is_some() || change.body.is_some() {
        Some(GitBucketPullRequestContent {
            title: change
                .title
                .as_deref()
                .or_else(|| current.as_ref().map(|pr| pr.title.as_str()))
                .ok_or_else(|| {
                    GbMcpError::Other("Cannot prepare a pull request title update".into())
                })?,
            body: change
                .body
                .as_deref()
                .or_else(|| current.as_ref().and_then(|pr| pr.body.as_deref())),
        })
    } else {
        None
    };
    let request = GitBucketPullRequestUpdate {
        state: change.state.as_deref(),
        base: change.base.as_deref(),
        content,
    };

    let route = client.operation_route(super::profile::Operation::PullRequestUpdate);
    let mut fields = Vec::new();
    if request.content.is_some() {
        fields.push(("title", change.title.is_some()));
        fields.push(("body", change.body.is_some()));
    }
    if change.state.is_some() {
        fields.push(("state", true));
    }
    if change.base.is_some() {
        fields.push(("base", true));
    }
    if fields.is_empty() {
        return Err(GbMcpError::InvalidInput(
            "Supply a pull request change".into(),
        ));
    }
    let operation = Operation::new(
        "update_pull_request",
        OperationTarget::new(ResourceKind::PullRequest, owner, repo).numbered(number),
        route,
        &fields,
    );
    operation
        .run(async {
            let path = format!("/repos/{owner}/{repo}/pulls/{number}");
            let pr: PullRequest = client.patch(&path, &request).await?;
            operation.identity(pr.number)?;
            verify(&operation, &request, &pr)?;
            Ok(pr)
        })
        .await
}

async fn validate_base(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    base: &str,
) -> Result<()> {
    super::url_safety::repository(owner, repo)?;
    let url =
        super::url_safety::append(client.base_url(), &["repos", owner, repo, "branches", base])?;
    let branch: Branch = client.get_url(url, "pull request base branch").await?;
    if branch.name != base {
        return Err(GbMcpError::Other(
            "GitBucket returned a different base branch; pull request was not updated".into(),
        ));
    }
    Ok(())
}

fn verify(
    operation: &Operation,
    request: &GitBucketPullRequestUpdate<'_>,
    pr: &PullRequest,
) -> Result<()> {
    let mut checks = Vec::new();
    if let Some(state) = request.state {
        checks.push(("state", state == pr.state));
    }
    if let Some(base) = request.base {
        checks.push((
            "base",
            pr.base.as_ref().map(|b| b.ref_name.as_str()) == Some(base),
        ));
    }
    if let Some(content) = &request.content {
        checks.push(("title", content.title == pr.title));
        checks.push((
            "body",
            content.body.unwrap_or_default() == pr.body.as_deref().unwrap_or_default(),
        ));
    }
    operation.verify(&checks)
}
