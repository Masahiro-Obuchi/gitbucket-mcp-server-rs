//! Pull request update orchestration, separate from HTTP request construction.

use super::client::GitBucketClient;
use crate::error::{GbMcpError, Result};
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

    client.operation_route(super::profile::Operation::PullRequestUpdate);
    let path = format!("/repos/{owner}/{repo}/pulls/{number}");
    let pr = client.patch(&path, &request).await?;
    verify(&request, &pr)?;
    Ok(pr)
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

fn verify(request: &GitBucketPullRequestUpdate<'_>, pr: &PullRequest) -> Result<()> {
    let mut fields = Vec::new();
    if request.state.is_some_and(|state| state != pr.state) {
        fields.push("state");
    }
    if request
        .base
        .is_some_and(|base| pr.base.as_ref().map(|b| b.ref_name.as_str()) != Some(base))
    {
        fields.push("base");
    }
    if let Some(content) = &request.content {
        if content.title != pr.title {
            fields.push("title");
        }
        // GitBucket serializes an absent issue body as an empty string.
        if content.body.unwrap_or_default() != pr.body.as_deref().unwrap_or_default() {
            fields.push("body");
        }
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(GbMcpError::OperationNotVerified {
            operation: "Pull request update",
            fields,
        })
    }
}
