//! Verified optional creation fields. References are checked before creation;
//! requested metadata is observed again after the one POST, without rollback.
use std::collections::BTreeSet;

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::{
    error::{GbMcpError, Result},
    models::{
        issue::{CreateIssue, Issue},
        issue_label::LabelNames,
        operation::{OperationTarget, ResourceKind},
        pull_request::{CreatePullRequest, PullRequest},
    },
};

pub(crate) fn validate_issue(change: &CreateIssue) -> Result<()> {
    if let Some(number) = change.milestone {
        if number == 0 || number > i32::MAX as u64 {
            return Err(GbMcpError::InvalidInput(
                "milestone must be between 1 and 2147483647".into(),
            ));
        }
    }
    if let Some(labels) = &change.labels {
        super::issue_label::names(
            &LabelNames {
                labels: labels.clone(),
            },
            true,
        )?;
    }
    if let Some(assignees) = &change.assignees {
        let mut names = BTreeSet::new();
        for name in assignees {
            super::url_safety::identifier(name, "username")?;
            if !names.insert(name) {
                return Err(GbMcpError::InvalidInput(
                    "assignees must not contain duplicate usernames".into(),
                ));
            }
        }
    }
    Ok(())
}

fn same_names<'a>(actual: impl Iterator<Item = &'a str>, expected: &[String]) -> bool {
    let actual: Vec<_> = actual.collect();
    let distinct: BTreeSet<_> = actual.iter().copied().collect();
    distinct.len() == actual.len() && distinct == expected.iter().map(String::as_str).collect()
}

pub(super) async fn issue(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    change: &CreateIssue,
) -> Result<Issue> {
    validate_issue(change)?;
    let fields: Vec<_> = [
        ("labels", change.labels.is_some()),
        ("assignees", change.assignees.is_some()),
        ("milestone", change.milestone.is_some()),
    ]
    .into_iter()
    .filter(|(_, supplied)| *supplied)
    .collect();
    let operation = Operation::new(
        "create_issue",
        OperationTarget::new(ResourceKind::Issue, owner, repo),
        Route::Rest,
        &fields,
    );
    operation.run(async {
        if let Some(number) = change.milestone {
            let milestone = client.get_milestone(owner, repo, number).await?;
            if milestone.number != number {
                return Err(GbMcpError::Other("GitBucket returned a different milestone; no Issue was created".into()));
            }
        }
        if let Some(labels) = &change.labels {
            for name in labels {
                if client.get_label(owner, repo, name).await?.name != *name {
                    return Err(GbMcpError::Other("GitBucket returned a different label; no Issue was created".into()));
                }
            }
        }
        if let Some(assignees) = &change.assignees {
            for name in assignees {
                if client.get_user(name).await?.login != *name {
                    return Err(GbMcpError::Other("GitBucket returned a different account; no Issue was created".into()));
                }
            }
        }
        let created: Issue = client.post(&format!("/repos/{owner}/{repo}/issues"), change).await?;
        operation.identity(created.number)?;
        if fields.is_empty() { return Ok(created); }
        let observed = client.get_issue(owner, repo, created.number).await?;
        operation.identity(observed.number)?;
        let mut checks = Vec::new();
        let mut missing = false;
        if let Some(expected) = &change.labels {
            if let Some(actual) = &observed.labels { checks.push(("labels", same_names(actual.iter().map(|label| label.name.as_str()), expected))); }
            else { missing = true; }
        }
        if let Some(expected) = &change.assignees {
            if let Some(actual) = &observed.assignees { checks.push(("assignees", same_names(actual.iter().map(|user| user.login.as_str()), expected))); }
            else { missing = true; }
        }
        if let Some(expected) = change.milestone {
            if let Some(actual) = &observed.milestone { checks.push(("milestone", actual.number == expected)); }
            else { missing = true; }
        }
        operation.verify(&checks)?;
        if missing {
            return Err(GbMcpError::Other("Not all requested Issue creation metadata could be observed; inspect the created Issue before another write".into()));
        }
        Ok(observed)
    }).await
}

pub(super) async fn pull_request(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    change: &CreatePullRequest,
) -> Result<PullRequest> {
    let fields = if change.draft.is_some() {
        vec![("draft", true)]
    } else {
        Vec::new()
    };
    let operation = Operation::new(
        "create_pull_request",
        OperationTarget::new(ResourceKind::PullRequest, owner, repo),
        Route::Rest,
        &fields,
    );
    operation.run(async {
        let created: PullRequest = client.post(&format!("/repos/{owner}/{repo}/pulls"), change).await?;
        operation.identity(created.number)?;
        let Some(expected) = change.draft else { return Ok(created); };
        let observed = client.get_pull_request(owner, repo, created.number).await?;
        operation.identity(observed.number)?;
        let actual = observed.draft.ok_or_else(|| GbMcpError::Other("GitBucket did not expose the requested draft state; inspect the created PR before another write".into()))?;
        operation.verify(&[("draft", actual == expected)])?;
        Ok(observed)
    }).await
}
