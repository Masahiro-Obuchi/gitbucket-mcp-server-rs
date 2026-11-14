use crate::{
    server::GitBucketMcpServer,
    tools::{
        response::{from_gb_error, read_only_error, success, validation_error, ToolResult},
        validation::repository_fields,
    },
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetIssueMilestoneParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Existing milestone number in this repository, 1..2147483647; open and closed milestones are supported"
    )]
    pub milestone_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearIssueMilestoneParams {
    pub owner: String,
    pub repo: String,
    #[schemars(
        description = "Issue or PR number, 1..2147483647; clears only its assignment, retaining the milestone definition"
    )]
    pub issue_number: u64,
}

#[tool_router(router = tool_router_issue_milestone, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "Set an existing repository milestone on an Issue or PR. Requires standard profile; actual changes require matching PAT/Web credentials and developer permission. Checks target/reference, writes once via Web and verifies the milestone and preserved content/assignments. An already matching assignment is a read-only no-op. No retries or rollback",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn set_issue_milestone(
        &self,
        Parameters(params): Parameters<SetIssueMilestoneParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(value) => value,
            Err(error) => return validation_error(error),
        };
        if let Err(error) = crate::api::issue_label::number(params.issue_number)
            .and_then(|()| crate::api::issue_milestone::milestone_number(params.milestone_number))
        {
            return from_gb_error(error);
        }
        match self
            .client
            .set_issue_milestone(&owner, &repo, params.issue_number, params.milestone_number)
            .await
        {
            Ok(issue) => success(&issue),
            Err(error) => from_gb_error(error),
        }
    }

    #[tool(
        description = "Clear an Issue or PR milestone assignment and retain the milestone definition. Requires standard profile and matching PAT/Web credentials; actual changes require developer permission. Verifies the stored Issue and the native page's explicit No milestone marker; absent REST data alone is insufficient. Already clear is a verified no-op. No retries or rollback",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn clear_issue_milestone(
        &self,
        Parameters(params): Parameters<ClearIssueMilestoneParams>,
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
        match self
            .client
            .clear_issue_milestone(&owner, &repo, params.issue_number)
            .await
        {
            Ok(issue) => success(&issue),
            Err(error) => from_gb_error(error),
        }
    }
}
