//! Label update intent, route selection and result verification.
use super::{
    client::GitBucketClient,
    profile::{Operation, Route},
};
use crate::{
    error::{GbMcpError, Result},
    models::label::{Label, UpdateLabel},
};
use serde::Serialize;

const UPDATE: &str = "Label update";

#[derive(Serialize)]
struct LabelWrite<'a> {
    name: &'a str,
    color: &'a str,
}

pub(super) fn color(value: &str) -> Result<String> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(GbMcpError::InvalidInput(
            "Label color must be six hexadecimal digits".into(),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

pub(super) fn name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.encode_utf16().count() > 100
        || value.starts_with(['-', '_'])
        || matches!(value, "." | "..")
        || value.chars().any(char::is_control)
    {
        return Err(GbMcpError::InvalidInput("Label name must be 1-100 UTF-16 code units, without controls, a leading hyphen/underscore or a dot segment".into()));
    }
    Ok(())
}

pub(super) async fn update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    old_name: &str,
    change: &UpdateLabel,
) -> Result<Label> {
    if change.new_name.is_none() && change.color.is_none() {
        return Err(GbMcpError::InvalidInput(
            "At least one of new_name or color is required".into(),
        ));
    }
    let next_name = change.new_name.as_deref().unwrap_or(old_name);
    name(next_name)?;
    let supplied_color = change.color.as_deref().map(color).transpose()?;
    let current = if change.color.is_none() || next_name == old_name {
        let current = client.get_label(owner, repo, old_name).await?;
        if current.name != old_name {
            return Err(GbMcpError::Other(
                "GitBucket returned a different label; no update was sent".into(),
            ));
        }
        Some(current)
    } else {
        None
    };
    let next_color = supplied_color
        .or_else(|| current.as_ref().and_then(|label| label.color.clone()))
        .ok_or_else(|| {
            GbMcpError::Other(
                "The current label color could not be fetched; no update was sent".into(),
            )
        })?;
    let next_color = color(&next_color)?;
    if let Some(current) = &current {
        if current.name == next_name
            && current
                .color
                .as_deref()
                .and_then(|c| color(c).ok())
                .as_deref()
                == Some(&next_color)
        {
            return Ok(current.clone());
        }
    }
    let operation = if next_name == old_name {
        Operation::LabelSameNameUpdate
    } else {
        Operation::LabelRename
    };
    let result = if client.operation_route(operation) == Route::Web {
        web_update(client, owner, repo, old_name, next_name, &next_color).await?
    } else {
        let url = super::label::label_url(client, owner, repo, old_name)?;
        client
            .patch_url(
                url,
                "label update",
                &LabelWrite {
                    name: next_name,
                    color: &next_color,
                },
            )
            .await?
    };
    verify(&result, next_name, &next_color, UPDATE)?;
    Ok(result)
}

pub(super) fn verify(
    label: &Label,
    expected_name: &str,
    expected_color: &str,
    operation: &'static str,
) -> Result<()> {
    let mut fields = Vec::new();
    if label.name != expected_name {
        fields.push("name");
    }
    if label
        .color
        .as_deref()
        .and_then(|c| color(c).ok())
        .as_deref()
        != Some(expected_color)
    {
        fields.push("color");
    }
    if fields.is_empty() {
        Ok(())
    } else {
        Err(GbMcpError::OperationNotVerified { operation, fields })
    }
}

async fn web_update(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    old_name: &str,
    next_name: &str,
    next_color: &str,
) -> Result<Label> {
    if next_name.contains(',') {
        return Err(GbMcpError::Unsupported("The GitBucket web label form cannot preserve a comma in a label name; no web write was sent".into()));
    }
    if next_name != next_name.trim() {
        return Err(GbMcpError::Unsupported("The GitBucket web label form cannot preserve leading or trailing whitespace in a label name; no web write was sent".into()));
    }
    let session = client.web_session().await?;
    let label_id = session.find_label_id(owner, repo, old_name).await?;
    session
        .edit_label(owner, repo, label_id, next_name, next_color)
        .await?;
    client
        .get_label(owner, repo, next_name)
        .await
        .map_err(|e| super::http::unknown(UPDATE, e))
}
