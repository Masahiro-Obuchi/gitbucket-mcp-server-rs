//! The verified native draft -> ready operation; no reverse transition or merge.
use std::collections::BTreeSet;

use super::{
    client::GitBucketClient,
    outcome::Operation,
    profile::{ApiProfile, Route},
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        operation::{OperationTarget, ResourceKind},
        pull_request::{canonical_sha, PullRequest, PullRequestHead},
    },
};

pub(crate) fn number(number: u64) -> Result<()> {
    if number == 0 || number > i32::MAX as u64 {
        return Err(GbMcpError::InvalidInput(
            "pull_number must be between 1 and 2147483647".into(),
        ));
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct Revision {
    repository: String,
    branch: String,
    sha: String,
}

fn revision(value: &Option<PullRequestHead>) -> Option<Revision> {
    let value = value.as_ref()?;
    let repository = value.repo.as_ref()?;
    let (owner, repo) = repository.full_name.split_once('/')?;
    if super::url_safety::repository(owner, repo).is_err()
        || repository.name != repo
        || value.ref_name.trim().is_empty()
        || value.ref_name.chars().any(char::is_control)
    {
        return None;
    }
    let sha = value.sha.as_ref().filter(|sha| canonical_sha(sha))?;
    Some(Revision {
        repository: repository.full_name.clone(),
        branch: value.ref_name.clone(),
        sha: sha.clone(),
    })
}

fn names(values: impl Iterator<Item = String>) -> Option<BTreeSet<String>> {
    let values: Vec<_> = values.collect();
    let unique: BTreeSet<_> = values.iter().cloned().collect();
    (values.len() == unique.len()).then_some(unique)
}
fn labels(pr: &PullRequest) -> Option<BTreeSet<String>> {
    names(pr.labels.as_ref()?.iter().map(|label| label.name.clone()))
}
fn assignees(pr: &PullRequest) -> Option<BTreeSet<String>> {
    let users = pr.assignees.as_ref()?;
    if users
        .iter()
        .any(|user| super::url_safety::identifier(&user.login, "username").is_err())
    {
        return None;
    }
    names(users.iter().map(|user| user.login.clone()))
}

fn baseline(pr: &PullRequest, owner: &str, repo: &str, number: u64) -> Result<()> {
    if pr.number != number {
        return Err(GbMcpError::Other(
            "GitBucket returned a different pull request; no ready transition was sent".into(),
        ));
    }
    if pr.state != "open" || pr.merged != Some(false) {
        return Err(GbMcpError::Unsupported("Ready transition requires an open, explicitly unmerged pull request; no write was sent".into()));
    }
    let base = revision(&pr.base);
    if pr.draft.is_none()
        || pr.body.is_none()
        || labels(pr).is_none()
        || assignees(pr).is_none()
        || revision(&pr.head).is_none()
        || base
            .as_ref()
            .is_none_or(|base| base.repository != format!("{owner}/{repo}"))
    {
        return Err(GbMcpError::Unsupported("Cannot observe the draft state and complete PR fields to preserve; no ready transition was sent".into()));
    }
    Ok(())
}

fn preservation(before: &PullRequest, after: &PullRequest) -> Vec<(&'static str, bool)> {
    let mut checks = vec![
        ("title", before.title == after.title),
        ("state", before.state == after.state),
    ];
    if let Some(body) = &after.body {
        checks.push(("body", Some(body) == before.body.as_ref()));
    }
    if let Some(merged) = after.merged {
        checks.push(("merged", Some(merged) == before.merged));
    }
    if let Some(actual) = revision(&after.head) {
        checks.push(("head", Some(actual) == revision(&before.head)));
    }
    if let Some(actual) = revision(&after.base) {
        checks.push(("base", Some(actual) == revision(&before.base)));
    }
    if let Some(actual) = labels(after) {
        checks.push(("labels", Some(actual) == labels(before)));
    }
    if let Some(actual) = assignees(after) {
        checks.push(("assignees", Some(actual) == assignees(before)));
    }
    checks
}

pub(super) async fn ready(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequest> {
    super::url_safety::repository(owner, repo)?;
    self::number(number)?;
    if client.api_profile() != ApiProfile::Standard {
        return Err(GbMcpError::Unsupported("Marking a PR ready requires the standard profile's verified Web operation; rest-only sends no request".into()));
    }
    let before = client.get_pull_request(owner, repo, number).await?;
    baseline(&before, owner, repo, number)?;
    if before.draft == Some(false) {
        return Ok(before);
    }
    let mut target = OperationTarget::new(ResourceKind::PullRequest, owner, repo).numbered(number);
    target.commit = Some(revision(&before.head).expect("validated head").sha);
    let operation = Operation::new(
        "mark_pull_request_ready",
        target,
        Route::Web,
        &[
            ("draft", true),
            ("title", false),
            ("body", false),
            ("state", false),
            ("merged", false),
            ("head", false),
            ("base", false),
            ("labels", false),
            ("assignees", false),
        ],
    );
    operation.run(async {
        let session = client.web_session().await?;
        // Establish the same revision/content after sign-in, before the cookie
        // check immediately preceding the single native POST. This is not CAS.
        let current = client.get_pull_request(owner, repo, number).await?;
        baseline(&current, owner, repo, number)?;
        let checks = preservation(&before, &current);
        if checks.len() != 8 || checks.iter().any(|(_, matched)| !matched) {
            return Err(GbMcpError::ReadConflict("PR content, revision or assignments changed before marking ready; no write was sent"));
        }
        if current.draft == Some(false) { return Ok(current); }
        session.mark_pull_request_ready(owner, repo, number).await?;
        let after = client.get_pull_request(owner, repo, number).await?;
        operation.identity(after.number)?;
        let mut checks = preservation(&before, &after);
        if let Some(draft) = after.draft { checks.push(("draft", !draft)); }
        operation.verify(&checks)?;
        if checks.len() != 9 {
            return Err(GbMcpError::Other("Not all draft/retained PR fields could be observed; inspect the PR before another write".into()));
        }
        Ok(after)
    }).await
}
