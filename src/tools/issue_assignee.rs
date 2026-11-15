use crate::{
    models::issue_assignee::AssigneeNames,
    server::GitBucketMcpServer,
    tools::{
        response::{from_gb_error, read_only_error, success, validation_error, ToolResult},
        validation::{repository_fields, required_trimmed},
    },
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeIssueAssigneesParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Distinct existing individual account logins; identifiers are trimmed. Add needs a nonempty array; replace with [] clears all assignments"
    )]
    pub assignees: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveIssueAssigneeParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Existing individual account login to unassign; identifier is trimmed"
    )]
    pub username: String,
}

fn normalized(
    params: &ChangeIssueAssigneesParams,
    allow_empty: bool,
) -> crate::error::Result<AssigneeNames> {
    let change = AssigneeNames {
        assignees: params
            .assignees
            .iter()
            .map(|name| {
                required_trimmed(name, "assignee").map_err(crate::error::GbMcpError::InvalidInput)
            })
            .collect::<crate::error::Result<Vec<_>>>()?,
    };
    crate::api::issue_assignee::names(&change, allow_empty)?;
    Ok(change)
}

#[tool_router(router = tool_router_issue_assignee, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "Add missing individual assignees to an Issue or PR, retaining existing ones. Requires standard profile and matching PAT/Web credentials even for no-ops; actual changes need developer permission. Validate references and complete REST/Web sets; verify each one-person POST and preserved fields. A prefix can survive failure: inspect target and per-account evidence; no retries or rollback",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn add_issue_assignees(
        &self,
        Parameters(params): Parameters<ChangeIssueAssigneesParams>,
    ) -> ToolResult {
        self.change_assignees(params, false).await
    }

    #[tool(
        description = "Replace all Issue/PR individual assignees; [] clears the set. Requires standard profile, matching PAT/Web credentials even for no-ops and developer permission for changes. Adds missing accounts before removing extras, checking each stored set and preserved fields. This batch is sequential, not atomic; inspect prefix evidence after failure. No retries, fallback or rollback",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn replace_issue_assignees(
        &self,
        Parameters(params): Parameters<ChangeIssueAssigneesParams>,
    ) -> ToolResult {
        self.change_assignees(params, true).await
    }

    async fn change_assignees(
        &self,
        params: ChangeIssueAssigneesParams,
        replace: bool,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(value) => value,
            Err(error) => return validation_error(error),
        };
        if let Err(error) = crate::api::issue_label::number(params.issue_number) {
            return from_gb_error(error);
        }
        let change = match normalized(&params, replace) {
            Ok(value) => value,
            Err(error) => return from_gb_error(error),
        };
        let result = if replace {
            self.client
                .replace_issue_assignees(&owner, &repo, params.issue_number, &change)
                .await
        } else {
            self.client
                .add_issue_assignees(&owner, &repo, params.issue_number, &change)
                .await
        };
        match result {
            Ok(issue) => success(&issue),
            Err(error) => from_gb_error(error),
        }
    }

    #[tool(
        description = "Remove one individual assignee from an Issue or PR, retaining all others. Requires standard profile and matching PAT/Web credentials even for an already absent no-op; actual changes need developer permission. Validate the exact account and complete REST/Web sets, then verify the one POST and preserved fields. No retry, redirect, fallback or rollback",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn remove_issue_assignee(
        &self,
        Parameters(params): Parameters<RemoveIssueAssigneeParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(value) => value,
            Err(error) => return validation_error(error),
        };
        if let Err(error) = crate::api::issue_label::number(params.issue_number) {
            return from_gb_error(error);
        }
        let username = match required_trimmed(&params.username, "username") {
            Ok(value) => value,
            Err(error) => return validation_error(error),
        };
        if let Err(error) = crate::api::url_safety::identifier(&username, "username") {
            return from_gb_error(error);
        }
        match self
            .client
            .remove_issue_assignee(&owner, &repo, params.issue_number, &username)
            .await
        {
            Ok(issue) => success(&issue),
            Err(error) => from_gb_error(error),
        }
    }
}
