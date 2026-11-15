//! Verified individual account assignments on Issues and PRs.
use std::collections::BTreeSet;

use super::{
    client::GitBucketClient,
    outcome::Operation,
    profile::{ApiProfile, Route},
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        issue::Issue,
        issue_assignee::AssigneeNames,
        operation::{OperationTarget, ResourceKind},
    },
};

pub(crate) fn names(change: &AssigneeNames, allow_empty: bool) -> Result<()> {
    if change.assignees.is_empty() && !allow_empty {
        return Err(GbMcpError::InvalidInput(
            "assignees must contain at least one account login".into(),
        ));
    }
    let mut unique = BTreeSet::new();
    for name in &change.assignees {
        super::url_safety::identifier(name, "username")?;
        if !unique.insert(name) {
            return Err(GbMcpError::InvalidInput(
                "assignees must not contain duplicate account logins".into(),
            ));
        }
    }
    Ok(())
}

fn unique(names: impl Iterator<Item = String>) -> Option<BTreeSet<String>> {
    let values: Vec<_> = names.collect();
    let set: BTreeSet<_> = values.iter().cloned().collect();
    (set.len() == values.len()).then_some(set)
}

fn assignees(issue: &Issue) -> Option<BTreeSet<String>> {
    let users = issue.assignees.as_ref()?;
    if users.iter().any(|user| {
        user.user_type.as_deref() != Some("User")
            || super::url_safety::identifier(&user.login, "username").is_err()
    }) {
        return None;
    }
    unique(users.iter().map(|user| user.login.clone()))
}
fn labels(issue: &Issue) -> Option<BTreeSet<String>> {
    unique(
        issue
            .labels
            .as_ref()?
            .iter()
            .map(|label| label.name.clone()),
    )
}

enum Change<'a> {
    Add(&'a AssigneeNames),
    Replace(&'a AssigneeNames),
    Remove(&'a str),
}

struct Delta {
    name: String,
    add: bool,
    field: String,
}

impl GitBucketClient {
    pub async fn add_issue_assignees(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        change: &AssigneeNames,
    ) -> Result<Issue> {
        names(change, false)?;
        self.change_issue_assignees(owner, repo, number, Change::Add(change))
            .await
    }
    pub async fn replace_issue_assignees(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        change: &AssigneeNames,
    ) -> Result<Issue> {
        names(change, true)?;
        self.change_issue_assignees(owner, repo, number, Change::Replace(change))
            .await
    }
    pub async fn remove_issue_assignee(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        username: &str,
    ) -> Result<Issue> {
        super::url_safety::identifier(username, "username")?;
        self.change_issue_assignees(owner, repo, number, Change::Remove(username))
            .await
    }

    async fn change_issue_assignees(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        change: Change<'_>,
    ) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        super::issue_label::number(number)?;
        if self.api_profile() != ApiProfile::Standard {
            return Err(GbMcpError::Unsupported("Issue/PR assignee updates require the standard profile's verified Web contract; rest-only sends no request".into()));
        }
        let before = self.get_issue(owner, repo, number).await?;
        if before.number != number {
            return Err(GbMcpError::Other(
                "GitBucket returned a different Issue/PR; no assignee write was sent".into(),
            ));
        }
        let current = assignees(&before).ok_or_else(|| {
            GbMcpError::Unsupported(
                "Cannot observe a complete distinct set of individual assignees; no write was sent"
                    .into(),
            )
        })?;
        if before.body.is_none() || labels(&before).is_none() {
            return Err(GbMcpError::Unsupported(
                "Cannot observe the content and labels to preserve; no assignee write was sent"
                    .into(),
            ));
        }
        let (operation_name, references, expected): (_, Vec<&str>, BTreeSet<String>) = match change
        {
            Change::Add(request) => (
                "add_issue_assignees",
                request.assignees.iter().map(String::as_str).collect(),
                current
                    .iter()
                    .chain(request.assignees.iter())
                    .cloned()
                    .collect(),
            ),
            Change::Replace(request) => (
                "replace_issue_assignees",
                request.assignees.iter().map(String::as_str).collect(),
                request.assignees.iter().cloned().collect(),
            ),
            Change::Remove(name) => (
                "remove_issue_assignee",
                vec![name],
                current
                    .iter()
                    .filter(|value| value.as_str() != name)
                    .cloned()
                    .collect(),
            ),
        };
        for name in references {
            let user = self.get_user(name).await?;
            if user.login != name || user.user_type.as_deref() != Some("User") {
                return Err(GbMcpError::Unsupported("The requested assignee was not the exact individual account; no write was sent".into()));
            }
        }
        let session = self.web_session().await?;
        let is_pr = before.pull_request.is_some();
        let (web_names, no_milestone) = session
            .issue_assignment_state(owner, repo, number, is_pr)
            .await?;
        // REST hides assignments to removed accounts. The native sidebar must
        // independently enumerate exactly the same complete, canonical set.
        if web_names != current || (before.milestone.is_none() && !no_milestone) {
            return Err(GbMcpError::Unsupported("REST and Web assignment observations are incomplete or inconsistent; no write was sent".into()));
        }
        if current == expected {
            return Ok(before);
        }

        // Add before removing, each in canonical order. Never remove an old
        // assignee to rescue a failed addition or replay an ambiguous request.
        let delta: Vec<_> = expected
            .difference(&current)
            .map(|name| (name.clone(), true))
            .chain(
                current
                    .difference(&expected)
                    .map(|name| (name.clone(), false)),
            )
            .map(|(name, add)| Delta {
                field: format!("assignee.{name}"),
                name,
                add,
            })
            .collect();
        let mut fields = vec![
            ("assignees", true),
            ("title", false),
            ("body", false),
            ("state", false),
            ("labels", false),
            ("milestone", false),
        ];
        fields.extend(delta.iter().map(|item| (item.field.as_str(), true)));
        let operation = Operation::new(
            operation_name,
            OperationTarget::new(ResourceKind::Issue, owner, repo).numbered(number),
            Route::Web,
            &fields,
        );
        operation.run(async {
            let mut step_expected = current;
            let mut observed = before.clone();
            for (index, item) in delta.iter().enumerate() {
                operation.step(&["assignees", "title", "body", "state", "labels", "milestone", &item.field],
                    session.change_issue_assignee(owner, repo, number, &item.name, item.add)).await?;
                if item.add { step_expected.insert(item.name.clone()); }
                else { step_expected.remove(&item.name); }
                observed = self.get_issue(owner, repo, number).await?;
                operation.identity(observed.number)?;
                let mut checks = vec![("title", observed.title == before.title), ("state", observed.state == before.state)];
                let mut missing = false;
                if let Some(body) = &observed.body { checks.push(("body", Some(body) == before.body.as_ref())); }
                else { missing = true; }
                if let Some(actual) = labels(&observed) { checks.push(("labels", Some(actual) == labels(&before))); }
                else { missing = true; }
                match (before.milestone.as_ref(), observed.milestone.as_ref()) {
                    (Some(a), Some(b)) => checks.push(("milestone", a.number == b.number)),
                    (None, Some(_)) => checks.push(("milestone", false)),
                    _ => {},
                }
                // Preserve known REST evidence if a subsequent Web read fails.
                let preservation = operation.verify(&checks);
                let (web_names, no_milestone) = session.issue_assignment_state(owner, repo, number, is_pr).await?;
                let milestone_verification = if observed.milestone.is_none() && no_milestone {
                    operation.verify(&[("milestone", before.milestone.is_none())])
                } else {
                    if observed.milestone.is_none() { missing = true; }
                    Ok(())
                };
                let actual = assignees(&observed).ok_or_else(|| GbMcpError::Other("Post-write assignees were incomplete or ambiguous; inspect the Issue/PR before another write".into()))?;
                if actual != web_names {
                    return Err(GbMcpError::Other("Post-write REST and Web assignee observations differ; inspect the Issue/PR before another write".into()));
                }
                let mut checks: Vec<_> = delta[..=index].iter().map(|applied| (applied.field.as_str(), actual.contains(&applied.name) == applied.add)).collect();
                if index + 1 == delta.len() || actual != step_expected {
                    checks.push(("assignees", actual == expected));
                }
                operation.verify(&checks)?;
                preservation?;
                milestone_verification?;
                if missing {
                    return Err(GbMcpError::Other("Not all preserved fields could be observed after the assignee change; no later writes were sent".into()));
                }
                if actual != step_expected {
                    return Err(GbMcpError::Other("Assignees changed outside the verified step; no later writes were sent".into()));
                }
            }
            Ok(observed)
        }).await
    }
}
