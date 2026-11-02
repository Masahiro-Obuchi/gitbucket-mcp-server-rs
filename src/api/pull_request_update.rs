//! Pull request update orchestration, separate from HTTP request construction.

use super::client::GitBucketClient;
use super::issue::IssueUpdateError;
use crate::error::{GbMcpError, Result};
use crate::models::issue::UpdateIssue;
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

    let path = format!("/repos/{owner}/{repo}/pulls/{number}");
    let pr = match client.patch(&path, &request).await {
        Ok(pr) => pr,
        Err(original @ GbMcpError::Api { status: 404, .. }) => {
            // Preserve legacy compatibility for this focused fix. Operation-
            // specific capability routing replaces this policy separately.
            match client.get_pull_request(owner, repo, number).await {
                Ok(_) => legacy_update(client, owner, repo, number, &request).await?,
                Err(GbMcpError::Api { status: 404, .. }) => return Err(original),
                Err(err) => return Err(err),
            }
        }
        Err(err) => return Err(after_write_error(err)),
    };
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
        Err(GbMcpError::UpdateNotVerified { fields })
    }
}

async fn legacy_update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    request: &GitBucketPullRequestUpdate<'_>,
) -> Result<PullRequest> {
    if request.base.is_some() {
        return Err(GbMcpError::Other(
            "This GitBucket instance does not support REST pull request updates, and fallback cannot update the base branch.".into(),
        ));
    }
    client
        .update_issue_with_outcome(
            owner,
            repo,
            number,
            &UpdateIssue {
                state: request.state.map(str::to_owned),
                title: request
                    .content
                    .as_ref()
                    .map(|content| content.title.to_owned()),
                body: request
                    .content
                    .as_ref()
                    .and_then(|content| content.body.map(str::to_owned)),
            },
        )
        .await
        .map_err(|error| match error {
            IssueUpdateError::BeforeWrite(cause) => cause,
            IssueUpdateError::Write(cause) => after_write_error(cause),
            IssueUpdateError::AfterWrite(cause) => GbMcpError::WriteOutcomeUnknown {
                cause: Box::new(cause),
            },
        })?;
    client
        .get_pull_request(owner, repo, number)
        .await
        .map_err(|cause| GbMcpError::WriteOutcomeUnknown {
            cause: Box::new(cause),
        })
}

fn after_write_error(error: GbMcpError) -> GbMcpError {
    let error = match error {
        GbMcpError::OperationOutcomeUnknown { cause, .. } => *cause,
        other => other,
    };
    match error {
        // A rejected request is distinct from losing the response to a write.
        err @ GbMcpError::Api {
            status: 400..=499, ..
        } if !matches!(&err, GbMcpError::Api { status: 408, .. }) => err,
        err => GbMcpError::WriteOutcomeUnknown {
            cause: Box::new(err),
        },
    }
}
