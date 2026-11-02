//! Translate caller changes into GitBucket's complete milestone write contract.
use std::collections::HashSet;

use super::client::GitBucketClient;
use crate::error::{GbMcpError, Result};
use crate::models::milestone::{
    CreateMilestone, FieldChange, GitBucketMilestoneWrite, Milestone, UpdateMilestone,
};
use crate::models::milestone_date::MilestoneDate;

const CREATE: &str = "Milestone creation";
const UPDATE: &str = "Milestone update";

struct Changes {
    title: Option<String>,
    state: Option<String>,
    description: FieldChange<String>,
    due_on: FieldChange<MilestoneDate>,
}

impl Changes {
    fn parse(change: &UpdateMilestone) -> Result<Self> {
        if change.title.is_none()
            && change.state.is_none()
            && change.description.is_none()
            && change.due_on.is_none()
        {
            return Err(GbMcpError::InvalidInput(
                "At least one milestone change must be supplied".into(),
            ));
        }
        let title = change.title.as_deref().map(valid_title).transpose()?;
        if let Some(state) = change.state.as_deref() {
            valid_state(state)?;
        }
        let description = match change.description.as_deref() {
            None => FieldChange::Keep,
            Some("") => FieldChange::Clear,
            Some(value) => FieldChange::Set(value.to_owned()),
        };
        let due_on = match change.due_on.as_deref().map(str::trim) {
            None => FieldChange::Keep,
            Some("") => FieldChange::Clear,
            Some(value) => FieldChange::Set(MilestoneDate::parse(value)?),
        };
        Ok(Self {
            title,
            state: change.state.clone(),
            description,
            due_on,
        })
    }

    fn apply(self, mut current: GitBucketMilestoneWrite) -> GitBucketMilestoneWrite {
        if let Some(title) = self.title {
            current.title = title;
        }
        if let Some(state) = self.state {
            current.state = state;
        }
        apply_field(self.description, &mut current.description);
        apply_field(self.due_on, &mut current.due_on);
        current
    }
}

fn apply_field<T>(change: FieldChange<T>, value: &mut Option<T>) {
    match change {
        FieldChange::Keep => {}
        FieldChange::Set(next) => *value = Some(next),
        FieldChange::Clear => *value = None,
    }
}

fn valid_title(title: &str) -> Result<String> {
    let title = title.trim();
    if title.is_empty() || title.encode_utf16().count() > 100 {
        return Err(GbMcpError::InvalidInput(
            "Milestone title must contain 1 to 100 characters".into(),
        ));
    }
    Ok(title.to_owned())
}

fn valid_state(state: &str) -> Result<()> {
    if matches!(state, "open" | "closed") {
        Ok(())
    } else {
        Err(GbMcpError::InvalidInput(
            "Milestone state must be open or closed".into(),
        ))
    }
}

fn existing(milestone: &Milestone) -> Result<GitBucketMilestoneWrite> {
    Ok(GitBucketMilestoneWrite {
        title: milestone.title.clone(),
        state: milestone.state.clone(),
        description: milestone.description.clone(),
        due_on: milestone
            .due_on
            .as_deref()
            .map(MilestoneDate::parse)
            .transpose()
            .map_err(|_| {
                GbMcpError::Other(
                    "GitBucket returned an invalid milestone due_on; no update was sent".into(),
                )
            })?,
    })
}

pub(super) async fn create(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    change: &CreateMilestone,
) -> Result<Milestone> {
    let expected = GitBucketMilestoneWrite {
        title: valid_title(&change.title)?,
        state: "open".into(),
        description: change.description.clone().filter(|s| !s.is_empty()),
        due_on: change
            .due_on
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(MilestoneDate::parse)
            .transpose()?,
    };
    let route = client.operation_route(
        if super::profile::restricted_milestone_title(&expected.title) {
            super::profile::Operation::MilestoneRestrictedTitle
        } else {
            super::profile::Operation::MilestoneWrite
        },
    );
    if route == super::profile::Route::Web {
        let result = web_create(client, owner, repo, &expected).await?;
        verify(&expected, &result, None, CREATE)?;
        return Ok(result);
    }
    let result = client
        .post(&format!("/repos/{owner}/{repo}/milestones"), &expected)
        .await?;
    verify(&expected, &result, None, CREATE)?;
    Ok(result)
}

pub(super) async fn update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    change: &UpdateMilestone,
) -> Result<Milestone> {
    // Validate caller input before even a preflight GET, especially Unicode dates.
    let changes = Changes::parse(change)?;
    let current = client.get_milestone(owner, repo, number).await?;
    if current.number != number {
        return Err(GbMcpError::Other(
            "GitBucket returned a different milestone; no update was sent".into(),
        ));
    }
    let before = existing(&current)?;
    let expected = changes.apply(before.clone());
    valid_state(&expected.state)?;
    if mismatched_fields(&expected, &current).is_empty() {
        return Ok(current);
    }
    let route = client.operation_route(
        if super::profile::restricted_milestone_title(&expected.title) {
            super::profile::Operation::MilestoneRestrictedTitle
        } else {
            super::profile::Operation::MilestoneWrite
        },
    );
    if route == super::profile::Route::Web {
        let result = web_update(client, owner, repo, number, &before, &expected).await?;
        verify(&expected, &result, Some(number), UPDATE)?;
        return Ok(result);
    }
    let result = client
        .patch(
            &format!("/repos/{owner}/{repo}/milestones/{number}"),
            &expected,
        )
        .await?;
    verify(&expected, &result, Some(number), UPDATE)?;
    Ok(result)
}

fn mismatched_fields(expected: &GitBucketMilestoneWrite, actual: &Milestone) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if expected.title != actual.title {
        fields.push("title");
    }
    if expected.state != actual.state {
        fields.push("state");
    }
    if expected.description.as_deref().unwrap_or_default()
        != actual.description.as_deref().unwrap_or_default()
    {
        fields.push("description");
    }
    match actual
        .due_on
        .as_deref()
        .map(MilestoneDate::parse)
        .transpose()
    {
        Ok(date) if date == expected.due_on => {}
        _ => fields.push("due_on"),
    }
    fields
}

fn verify(
    expected: &GitBucketMilestoneWrite,
    actual: &Milestone,
    number: Option<u64>,
    operation: &'static str,
) -> Result<()> {
    let mut fields = mismatched_fields(expected, actual);
    if number.is_some_and(|n| n != actual.number) {
        fields.push("number");
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(GbMcpError::OperationNotVerified { operation, fields })
    }
}

fn web_content(expected: &GitBucketMilestoneWrite) -> Result<Option<String>> {
    let description = expected.description.as_deref().unwrap_or_default();
    if description != description.trim() {
        return Err(GbMcpError::Unsupported("The GitBucket milestone web form cannot preserve description whitespace. No web write was sent.".into()));
    }
    expected.due_on.map(MilestoneDate::web_date).transpose()
}

async fn web_create(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    expected: &GitBucketMilestoneWrite,
) -> Result<Milestone> {
    let date = web_content(expected)?;
    let before: HashSet<_> = client
        .list_milestones(owner, repo, Some("all"))
        .await?
        .into_iter()
        .map(|m| m.number)
        .collect();
    let session = client.milestone_web_session().await?;
    session
        .create_milestone(
            owner,
            repo,
            &expected.title,
            expected.description.as_deref(),
            date.as_deref(),
        )
        .await
        .map_err(|err| unknown(CREATE, err))?;
    let candidates: Vec<_> = client
        .list_milestones(owner, repo, Some("all"))
        .await
        .map_err(|err| unknown(CREATE, err))?
        .into_iter()
        .filter(|m| !before.contains(&m.number) && m.title == expected.title)
        .collect();
    if candidates.len() != 1 {
        return Err(unknown(CREATE, GbMcpError::Other("Could not uniquely identify a newly created milestone; an existing same-title milestone is not a creation result".into())));
    }
    candidates.into_iter().next().ok_or_else(|| {
        unknown(
            CREATE,
            GbMcpError::Other("Created milestone was not found".into()),
        )
    })
}

async fn web_update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    before: &GitBucketMilestoneWrite,
    expected: &GitBucketMilestoneWrite,
) -> Result<Milestone> {
    let content_changed = before.title != expected.title
        || before.description.as_deref().unwrap_or_default()
            != expected.description.as_deref().unwrap_or_default()
        || before.due_on != expected.due_on;
    let date = if content_changed {
        web_content(expected)?
    } else {
        None
    };
    let session = client.milestone_web_session().await?;
    if content_changed {
        session
            .edit_milestone(
                owner,
                repo,
                number,
                &expected.title,
                expected.description.as_deref(),
                date.as_deref(),
            )
            .await
            .map_err(|err| unknown(UPDATE, err))?;
    }
    if before.state != expected.state {
        let action = if expected.state == "closed" {
            "close"
        } else {
            "open"
        };
        session
            .update_milestone_state(owner, repo, number, action)
            .await
            .map_err(|err| unknown(UPDATE, err))?;
    }
    client
        .get_milestone(owner, repo, number)
        .await
        .map_err(|err| unknown(UPDATE, err))
}

fn unknown(operation: &'static str, cause: GbMcpError) -> GbMcpError {
    super::http::unknown(operation, cause)
}
