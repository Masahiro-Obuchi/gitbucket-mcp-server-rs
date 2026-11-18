//! One native comment POST with a close/reopen action, never two writes.
use std::collections::BTreeSet;

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::{
    error::{GbMcpError, Result},
    models::{
        comment::{CommentStateChange, CreateComment},
        issue::Issue,
        operation::{OperationTarget, ResourceKind},
        pull_request::{canonical_sha, PullRequest, PullRequestHead},
    },
};

#[derive(Clone, Copy)]
pub(super) enum Action {
    Close,
    Reopen,
}
impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Close => "close_issue_with_comment",
            Self::Reopen => "reopen_issue_with_comment",
        }
    }
    fn query(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Reopen => "reopen",
        }
    }
    fn before(self) -> &'static str {
        match self {
            Self::Close => "open",
            Self::Reopen => "closed",
        }
    }
    fn after(self) -> &'static str {
        match self {
            Self::Close => "closed",
            Self::Reopen => "open",
        }
    }
}

pub(crate) fn validate(number: u64, body: &str) -> Result<()> {
    if number == 0 || number > i32::MAX as u64 || body.trim().is_empty() {
        return Err(GbMcpError::InvalidInput(
            "Supply issue_number in 1..2147483647 and a nonblank comment body".into(),
        ));
    }
    Ok(())
}

fn distinct(values: impl Iterator<Item = String>) -> Option<BTreeSet<String>> {
    let values: Vec<_> = values.collect();
    let names: BTreeSet<_> = values.iter().cloned().collect();
    (names.len() == values.len()).then_some(names)
}
fn labels(issue: &Issue) -> Option<BTreeSet<String>> {
    distinct(issue.labels.as_ref()?.iter().map(|x| x.name.clone()))
}
fn assignees(issue: &Issue) -> Option<BTreeSet<String>> {
    let users = issue.assignees.as_ref()?;
    if users
        .iter()
        .any(|u| super::url_safety::identifier(&u.login, "username").is_err())
    {
        return None;
    }
    distinct(users.iter().map(|u| u.login.clone()))
}
fn revision(value: &Option<PullRequestHead>) -> Option<(&str, &str, &str)> {
    let value = value.as_ref()?;
    let repo = value.repo.as_ref()?;
    let (owner, name) = repo.full_name.split_once('/')?;
    if super::url_safety::repository(owner, name).is_err()
        || repo.name != name
        || value.ref_name.trim().is_empty()
        || value.ref_name.chars().any(char::is_control)
    {
        return None;
    }
    Some((
        &repo.full_name,
        &value.ref_name,
        value.sha.as_deref().filter(|sha| canonical_sha(sha))?,
    ))
}

const ISSUE_FIELDS: &[&str] = &[
    "state",
    "title",
    "body",
    "labels",
    "assignees",
    "kind",
    "milestone",
];
const PR_FIELDS: &[&str] = &[
    "pull_request.state",
    "pull_request.merged",
    "pull_request.draft",
    "pull_request.head",
    "pull_request.base",
];

pub(super) async fn change(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    body: &CreateComment,
    action: Action,
) -> Result<CommentStateChange> {
    super::url_safety::repository(owner, repo)?;
    validate(number, &body.body)?;
    let before = client.get_issue(owner, repo, number).await?;
    if before.number != number {
        return Err(GbMcpError::Other(
            "GitBucket returned a different Issue; no comment or state change was sent".into(),
        ));
    }
    if before.state != action.before() {
        return Err(GbMcpError::Unsupported("The Issue/PR is not in the required starting state; no comment or state change was sent".into()));
    }
    if before.body.is_none() || labels(&before).is_none() || assignees(&before).is_none() {
        return Err(GbMcpError::Unsupported(
            "Cannot observe the Issue fields to preserve; no write was sent".into(),
        ));
    }
    let pr_before = if before.pull_request.is_some() {
        let pr = client.get_pull_request(owner, repo, number).await?;
        if pr.number != number
            || pr.state != before.state
            || pr.merged != Some(false)
            || pr.draft.is_none()
            || revision(&pr.head).is_none()
            || revision(&pr.base).is_none_or(|r| r.0 != format!("{owner}/{repo}"))
        {
            return Err(GbMcpError::Unsupported(
                "The PR is merged, unknown or has incomplete revisions; no write was sent".into(),
            ));
        }
        Some(pr)
    } else {
        None
    };
    let actor = client.get_authenticated_user().await?;
    super::url_safety::identifier(&actor.login, "authenticated user")?;
    let mut fields = vec![
        ("comment.body", true),
        ("comment.author", true),
        ("comment.parent", true),
    ];
    fields.extend(
        ISSUE_FIELDS
            .iter()
            .filter(|f| **f != "milestone" || before.milestone.is_some())
            .map(|f| (*f, *f == "state")),
    );
    if pr_before.is_some() {
        fields.extend(PR_FIELDS.iter().map(|f| (*f, *f == "pull_request.state")));
    }
    let mut target = OperationTarget::new(ResourceKind::IssueComment, owner, repo).parented(number);
    target.commit = pr_before
        .as_ref()
        .and_then(|p| p.head.as_ref()?.sha.clone());
    let operation = Operation::new(action.name(), target, Route::Rest, &fields);
    let future = async {
        let created_id = client
            .post_comment_state_once(
                &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
                action.query(),
                body,
            )
            .await?;
        // Capture the POST locator before any observation, including cancellation.
        operation.identity(created_id)?;
        super::issue::validate_comment_id(created_id)?;
        let mut checks = Vec::new();
        let mut error = None;
        let issue = match client.get_issue(owner, repo, number).await {
            Ok(issue) => match operation.parent_identity_for_fields(issue.number, ISSUE_FIELDS) {
                Ok(()) => {
                    observe_issue(&before, &issue, action, &mut checks);
                    Some(issue)
                }
                Err(e) => {
                    error = Some(e);
                    None
                }
            },
            Err(e) => {
                error = Some(e);
                None
            }
        };
        // Retain each independent observation across later read failures,
        // deadlines and cancellation. Mismatches do not prevent the other reads.
        let _ = operation.verify(&checks);
        let mut pull_request = None;
        if let Some(before) = &pr_before {
            match client.get_pull_request(owner, repo, number).await {
                Ok(pr) => match operation.parent_identity_for_fields(pr.number, PR_FIELDS) {
                    Ok(()) => {
                        observe_pr(before, &pr, action, &mut checks);
                        pull_request = Some(pr);
                    }
                    Err(e) => {
                        error.get_or_insert(e);
                    }
                },
                Err(e) => {
                    error.get_or_insert(e);
                }
            }
        }
        let _ = operation.verify(&checks);
        // The repository-wide single-comment getter has no parent ID. The
        // native unpaged parent collection establishes membership, not a URL.
        let mut comment = None;
        match client.list_issue_comments(owner, repo, number).await {
            Ok(comments) => {
                let matching: Vec<_> = comments
                    .into_iter()
                    .filter(|c| c.id == created_id)
                    .collect();
                if matching.len() == 1 {
                    let c = matching.into_iter().next().expect("one comment");
                    checks.push(("comment.parent", true));
                    if let Some(value) = &c.body {
                        checks.push(("comment.body", value == &body.body));
                    }
                    if let Some(user) = &c.user {
                        checks.push(("comment.author", user.login == actor.login));
                    }
                    comment = Some(c);
                } else if matching.is_empty() {
                    checks.push(("comment.parent", false));
                }
            }
            Err(e) => {
                error.get_or_insert(e);
            }
        }
        let verification = operation.verify(&checks);
        if let Some(error) = error {
            return Err(error);
        }
        verification?;
        if checks.len() != fields.len() {
            return Err(GbMcpError::Other("Not all comment/state/preserved fields could be observed; inspect the parent and created comment before another write".into()));
        }
        Ok(CommentStateChange {
            comment: comment.expect("verified comment"),
            issue: issue.expect("verified parent"),
            pull_request,
        })
    };
    operation.run(future).await
}

fn observe_issue(
    before: &Issue,
    after: &Issue,
    action: Action,
    checks: &mut Vec<(&'static str, bool)>,
) {
    if matches!(after.state.as_str(), "open" | "closed") {
        checks.push(("state", after.state == action.after()));
    }
    checks.push(("title", after.title == before.title));
    checks.push((
        "kind",
        after.pull_request.is_some() == before.pull_request.is_some(),
    ));
    if let Some(body) = &after.body {
        checks.push(("body", Some(body) == before.body.as_ref()));
    }
    if let Some(names) = labels(after) {
        checks.push(("labels", Some(names) == labels(before)));
    }
    if let Some(names) = assignees(after) {
        checks.push(("assignees", Some(names) == assignees(before)));
    }
    if let Some(milestone) = &before.milestone {
        if let Some(actual) = &after.milestone {
            checks.push(("milestone", actual.number == milestone.number));
        }
    }
}
fn observe_pr(
    before: &PullRequest,
    after: &PullRequest,
    action: Action,
    checks: &mut Vec<(&'static str, bool)>,
) {
    if matches!(after.state.as_str(), "open" | "closed") {
        checks.push(("pull_request.state", after.state == action.after()));
    }
    if let Some(merged) = after.merged {
        checks.push(("pull_request.merged", Some(merged) == before.merged));
    }
    if let Some(draft) = after.draft {
        checks.push(("pull_request.draft", Some(draft) == before.draft));
    }
    if let Some(head) = revision(&after.head) {
        checks.push(("pull_request.head", Some(head) == revision(&before.head)));
    }
    if let Some(base) = revision(&after.base) {
        checks.push(("pull_request.base", Some(base) == revision(&before.base)));
    }
}
