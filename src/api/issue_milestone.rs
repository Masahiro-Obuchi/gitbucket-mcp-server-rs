//! Repository-scoped Issue/PR milestone assignment through the standard Web contract.
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
        operation::{OperationTarget, ResourceKind},
    },
};

pub(crate) fn milestone_number(number: u64) -> Result<()> {
    if number == 0 || number > i32::MAX as u64 {
        return Err(GbMcpError::InvalidInput(
            "milestone_number must be between 1 and 2147483647".into(),
        ));
    }
    Ok(())
}

fn distinct<'a>(names: impl Iterator<Item = &'a str>) -> Option<BTreeSet<&'a str>> {
    let names: Vec<_> = names.collect();
    let set: BTreeSet<_> = names.iter().copied().collect();
    (set.len() == names.len()).then_some(set)
}

fn labels(issue: &Issue) -> Option<BTreeSet<&str>> {
    distinct(
        issue
            .labels
            .as_ref()?
            .iter()
            .map(|label| label.name.as_str()),
    )
}
fn assignees(issue: &Issue) -> Option<BTreeSet<&str>> {
    distinct(
        issue
            .assignees
            .as_ref()?
            .iter()
            .map(|user| user.login.as_str()),
    )
}

impl GitBucketClient {
    /// Assign an existing repository milestone to an Issue or PR.
    pub async fn set_issue_milestone(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        milestone_number: u64,
    ) -> Result<Issue> {
        self.change_issue_milestone(owner, repo, issue_number, Some(milestone_number))
            .await
    }

    /// Clear the milestone assignment without deleting its definition.
    pub async fn clear_issue_milestone(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<Issue> {
        self.change_issue_milestone(owner, repo, issue_number, None)
            .await
    }

    async fn change_issue_milestone(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        expected: Option<u64>,
    ) -> Result<Issue> {
        super::url_safety::repository(owner, repo)?;
        super::issue_label::number(issue_number)?;
        if let Some(number) = expected {
            milestone_number(number)?;
        }
        // These versions have no Issue PATCH and PR PATCH ignores milestone.
        // Refuse before all HTTP instead of probing a known ineffective write.
        if self.api_profile() != ApiProfile::Standard {
            return Err(GbMcpError::Unsupported("Issue/PR milestone updates require the standard profile's verified Web contract; rest-only sends no request".into()));
        }
        let operation = Operation::new(
            if expected.is_some() {
                "set_issue_milestone"
            } else {
                "clear_issue_milestone"
            },
            OperationTarget::new(ResourceKind::Issue, owner, repo).numbered(issue_number),
            Route::Web,
            &[
                ("milestone", true),
                ("title", false),
                ("body", false),
                ("state", false),
                ("labels", false),
                ("assignees", false),
            ],
        );
        operation
            .run(async {
                let before = self.get_issue(owner, repo, issue_number).await?;
                operation.identity(before.number)?;
                if before.body.is_none() || labels(&before).is_none() || assignees(&before).is_none() {
                    return Err(GbMcpError::Unsupported("Cannot observe the fields to preserve; no milestone write was sent".into()));
                }
                if let Some(number) = expected {
                    let milestone = self.get_milestone(owner, repo, number).await?;
                    if milestone.number != number {
                        return Err(GbMcpError::Other("GitBucket returned a different milestone; no assignment write was sent".into()));
                    }
                    if before.milestone.as_ref().is_some_and(|m| m.number == number) {
                        return Ok(before);
                    }
                }
                let session = self.web_session().await?;
                let is_pr = before.pull_request.is_some();
                if expected.is_none() && before.milestone.is_none() {
                    // REST omits an unassigned milestone, but omission alone is not
                    // proof of clear. Require the native page's explicit empty marker.
                    if session.issue_has_no_milestone(owner, repo, issue_number, is_pr).await? {
                        return Ok(before);
                    }
                }
                session.set_issue_milestone(owner, repo, issue_number, expected).await?;
                let after = self.get_issue(owner, repo, issue_number).await?;
                operation.identity(after.number)?;
                // Record all known field observations before reading Web for clear,
                // so a later read failure retains the available evidence.
                let mut checks = vec![
                    ("title", before.title == after.title),
                    ("state", before.state == after.state),
                ];
                let mut missing = false;
                if let Some(body) = &after.body {
                    checks.push(("body", Some(body) == before.body.as_ref()));
                } else {
                    missing = true;
                }
                if let Some(actual) = labels(&after) {
                    checks.push(("labels", Some(actual) == labels(&before)));
                } else {
                    missing = true;
                }
                if let Some(actual) = assignees(&after) {
                    checks.push(("assignees", Some(actual) == assignees(&before)));
                } else {
                    missing = true;
                }
                let needs_empty_page = expected.is_none() && after.milestone.is_none();
                match (expected, after.milestone.as_ref()) {
                    (Some(number), Some(actual)) => checks.push(("milestone", actual.number == number)),
                    (None, Some(_)) => checks.push(("milestone", false)),
                    (Some(_), None) => missing = true,
                    (None, None) => {}
                }
                // Keep known observations even if the page read fails, while still
                // observing clear when a preserved field has already mismatched.
                let verification = operation.verify(&checks);
                if needs_empty_page {
                    let empty = session.issue_has_no_milestone(owner, repo, issue_number, is_pr).await?;
                    operation.verify(&[("milestone", empty)])?;
                }
                verification?;
                if missing {
                    return Err(GbMcpError::Other("Not all milestone assignment or preserved fields could be observed; inspect the Issue/PR before another write".into()));
                }
                Ok(after)
        })
        .await
    }
}
