//! Issue/PR label assignments, distinct from repository label definitions.
use std::collections::BTreeSet;

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::{
    error::{GbMcpError, Result},
    models::{
        issue_label::LabelNames,
        label::Label,
        operation::{OperationTarget, ResourceKind},
    },
};

pub(crate) fn number(number: u64) -> Result<()> {
    if number == 0 || number > i32::MAX as u64 {
        return Err(GbMcpError::InvalidInput(
            "issue_number must be between 1 and 2147483647".into(),
        ));
    }
    Ok(())
}
pub(crate) fn name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name.encode_utf16().count() > 100
        || name.chars().any(char::is_control)
        || matches!(name, "." | "..")
    {
        return Err(GbMcpError::InvalidInput("Label names must be nonblank, at most 100 UTF-16 code units, without controls or dot segments".into()));
    }
    Ok(())
}
pub(crate) fn names(change: &LabelNames, allow_empty: bool) -> Result<()> {
    if !allow_empty && change.labels.is_empty() {
        return Err(GbMcpError::InvalidInput(
            "labels must contain at least one name".into(),
        ));
    }
    let mut unique = BTreeSet::new();
    for label in &change.labels {
        name(label)?;
        if !unique.insert(label) {
            return Err(GbMcpError::InvalidInput(
                "labels must not contain duplicate names".into(),
            ));
        }
    }
    Ok(())
}
fn observed_names(labels: &[Label]) -> Result<BTreeSet<String>> {
    let unique: BTreeSet<_> = labels.iter().map(|label| label.name.clone()).collect();
    if unique.len() != labels.len() {
        return Err(GbMcpError::Other(
            "GitBucket returned duplicate issue label names".into(),
        ));
    }
    Ok(unique)
}

impl GitBucketClient {
    /// Complete, unpaged REST observation after checking the parent exists.
    pub async fn list_issue_labels(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<Vec<Label>> {
        super::url_safety::repository(owner, repo)?;
        number(issue_number)?;
        let issue = self.get_issue(owner, repo, issue_number).await?;
        if issue.number != issue_number {
            return Err(GbMcpError::Other(
                "GitBucket returned a different Issue/PR number".into(),
            ));
        }
        let labels: Vec<Label> = self
            .get(&format!(
                "/repos/{owner}/{repo}/issues/{issue_number}/labels"
            ))
            .await?;
        observed_names(&labels)?;
        Ok(labels)
    }

    pub async fn add_issue_labels(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        change: &LabelNames,
    ) -> Result<Vec<Label>> {
        names(change, false)?;
        self.change_issue_labels(owner, repo, issue_number, Change::Add(change))
            .await
    }
    pub async fn replace_issue_labels(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        change: &LabelNames,
    ) -> Result<Vec<Label>> {
        names(change, true)?;
        self.change_issue_labels(owner, repo, issue_number, Change::Replace(change))
            .await
    }
    pub async fn remove_issue_label(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        label_name: &str,
    ) -> Result<Vec<Label>> {
        name(label_name)?;
        self.change_issue_labels(owner, repo, issue_number, Change::Remove(label_name))
            .await
    }
    async fn change_issue_labels(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        change: Change<'_>,
    ) -> Result<Vec<Label>> {
        super::url_safety::repository(owner, repo)?;
        number(issue_number)?;
        let operation_name = match change {
            Change::Add(_) => "add_issue_labels",
            Change::Replace(_) => "replace_issue_labels",
            Change::Remove(_) => "remove_issue_label",
        };
        let operation = Operation::new(
            operation_name,
            OperationTarget::new(ResourceKind::IssueLabels, owner, repo).numbered(issue_number),
            Route::Rest,
            &[("labels", true)],
        );
        operation
            .run(async {
                let before = self.list_issue_labels(owner, repo, issue_number).await?;
                let current = observed_names(&before)?;
                let expected: BTreeSet<String> = match change {
                    Change::Add(request) => current
                        .iter()
                        .chain(request.labels.iter())
                        .cloned()
                        .collect(),
                    Change::Replace(request) => request.labels.iter().cloned().collect(),
                    Change::Remove(label) => current
                        .iter()
                        .filter(|name| name.as_str() != label)
                        .cloned()
                        .collect(),
                };
                if expected == current {
                    operation.verify(&[("labels", true)])?;
                    return Ok(before);
                }
                let route = format!("/repos/{owner}/{repo}/issues/{issue_number}/labels");
                match change {
                    Change::Add(request) => {
                        // Upstream inserts without checking existing associations.
                        // Sending the intersection can cause a duplicate-key error.
                        let missing = LabelNames {
                            labels: request
                                .labels
                                .iter()
                                .filter(|name| !current.contains(*name))
                                .cloned()
                                .collect(),
                        };
                        let _: Vec<Label> = self.post(&route, &missing).await?;
                    }
                    Change::Replace(request) => {
                        let _: Vec<Label> = self.put(&route, request).await?;
                    }
                    Change::Remove(label) => {
                        let url = super::url_safety::append(
                            self.base_url(),
                            &[
                                "repos",
                                owner,
                                repo,
                                "issues",
                                &issue_number.to_string(),
                                "labels",
                                label,
                            ],
                        )?;
                        self.delete_url(url, "Issue/PR label assignment").await?;
                    }
                }
                // The write response is not a complete current label set (DELETE
                // even returns an internal model). Re-read the scoped parent and set.
                let after = self.list_issue_labels(owner, repo, issue_number).await?;
                operation.verify(&[("labels", observed_names(&after)? == expected)])?;
                Ok(after)
            })
            .await
    }
}

#[derive(Clone, Copy)]
enum Change<'a> {
    Add(&'a LabelNames),
    Replace(&'a LabelNames),
    Remove(&'a str),
}
