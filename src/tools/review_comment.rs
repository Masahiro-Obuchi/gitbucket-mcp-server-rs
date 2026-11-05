use crate::{
    models::review_comment::storage_id,
    server::GitBucketMcpServer,
    tools::{
        response::{from_gb_error, success, validation_error, ToolResult},
        validation::repository_fields,
    },
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPullRequestReviewCommentsParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Positive pull request number")]
    pub pull_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPullRequestReviewCommentParams {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    #[schemars(
        description = "Repository-scoped review ID, e.g. commit:123; IssueComment IDs are separate"
    )]
    pub comment_id: String,
}

#[tool_router(router = tool_router_review_comment, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "Read commit/file/line comments observed on a PR Web conversation page. Returns HTML and partial coverage, not complete stored history or raw Markdown. Requires standard profile and matching PAT/Web credentials.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn list_pull_request_review_comments(
        &self,
        Parameters(params): Parameters<ListPullRequestReviewCommentsParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(error) => return validation_error(error),
        };
        if params.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        match self
            .client
            .list_pull_request_review_comments(&owner, &repo, params.pull_number)
            .await
        {
            Ok(result) => success(&result),
            Err(error) => from_gb_error(error),
        }
    }

    #[tool(
        description = "Find commit:<ID> within the PR's observed Web page. status=not_observed with comment=null means outside this partial observation, not deleted or globally missing. Returns HTML, unknown original positions and coverage.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn get_pull_request_review_comment(
        &self,
        Parameters(params): Parameters<GetPullRequestReviewCommentParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(error) => return validation_error(error),
        };
        if params.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        if let Err(error) = storage_id(&params.comment_id) {
            return from_gb_error(error);
        }
        match self
            .client
            .get_pull_request_review_comment(&owner, &repo, params.pull_number, &params.comment_id)
            .await
        {
            Ok(result) => success(&result),
            Err(error) => from_gb_error(error),
        }
    }
}
