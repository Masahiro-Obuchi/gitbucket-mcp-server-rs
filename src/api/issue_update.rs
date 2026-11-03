//! Issue update intent and verification, independent of web response parsing.
use super::{
    client::GitBucketClient,
    outcome::Operation,
    profile::{Operation as Capability, Route},
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        issue::{Issue, UpdateIssue},
        operation::{OperationTarget, ResourceKind},
    },
};

pub(super) async fn update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    change: &UpdateIssue,
) -> Result<Issue> {
    let fields: Vec<_> = [
        ("title", change.title.is_some()),
        ("body", change.body.is_some()),
        ("state", change.state.is_some()),
    ]
    .into_iter()
    .filter(|(_, supplied)| *supplied)
    .collect();
    if fields.is_empty()
        || change
            .state
            .as_deref()
            .is_some_and(|s| !matches!(s, "open" | "closed"))
    {
        return Err(GbMcpError::InvalidInput(
            "Supply an Issue change with a valid open/closed state".into(),
        ));
    }
    let route = client.operation_route(Capability::IssueUpdate);
    let operation = Operation::new(
        "update_issue",
        OperationTarget::new(ResourceKind::Issue, owner, repo).numbered(number),
        route,
        &fields,
    );
    operation
        .run(async {
            if route == Route::Rest {
                let actual: Issue = client
                    .patch(&format!("/repos/{owner}/{repo}/issues/{number}"), change)
                    .await?;
                verify(
                    &operation,
                    &actual,
                    change.title.as_deref(),
                    change.body.as_deref(),
                    change.state.as_deref(),
                )?;
                return Ok(super::issue::issue_with_closed_at_fallback(actual));
            }
            let current = client.get_issue(owner, repo, number).await?;
            if current.number != number {
                return Err(GbMcpError::Other(
                    "GitBucket returned a different issue; no write was sent".into(),
                ));
            }
            let title = change.title.as_deref().filter(|v| *v != current.title);
            let body = change
                .body
                .as_deref()
                .filter(|v| *v != current.body.as_deref().unwrap_or_default());
            let action = change
                .state
                .as_deref()
                .filter(|v| *v != current.state)
                .map(|state| if state == "closed" { "close" } else { "reopen" });
            if title.is_none() && body.is_none() && action.is_none() {
                return Ok(current);
            }

            let session = client.web_session().await?;
            if let Some(title) = title {
                operation
                    .step(
                        &["title"],
                        session.edit_issue_title(owner, repo, number, title),
                    )
                    .await?;
                let actual = client.get_issue(owner, repo, number).await?;
                verify(&operation, &actual, Some(title), None, None)?;
            }
            if let Some(body) = body {
                operation
                    .step(
                        &["body"],
                        session.edit_issue_content(owner, repo, number, body),
                    )
                    .await?;
                let actual = client.get_issue(owner, repo, number).await?;
                verify(
                    &operation,
                    &actual,
                    change.title.as_deref(),
                    Some(body),
                    None,
                )?;
            }
            if let Some(action) = action {
                operation
                    .step(
                        &["state"],
                        session.update_issue_state(owner, repo, number, action),
                    )
                    .await?;
            }
            let actual = client.get_issue(owner, repo, number).await?;
            verify(
                &operation,
                &actual,
                change.title.as_deref(),
                change.body.as_deref(),
                change.state.as_deref(),
            )?;
            Ok(actual)
        })
        .await
}

fn verify(
    operation: &Operation,
    actual: &Issue,
    title: Option<&str>,
    body: Option<&str>,
    state: Option<&str>,
) -> Result<()> {
    operation.identity(actual.number)?;
    let checks: Vec<_> = [
        ("title", title.map(|v| v == actual.title)),
        (
            "body",
            body.map(|v| v == actual.body.as_deref().unwrap_or_default()),
        ),
        ("state", state.map(|v| v == actual.state)),
    ]
    .into_iter()
    .filter_map(|(name, matched)| matched.map(|matched| (name, matched)))
    .collect();
    operation.verify(&checks)
}
