use crate::models::list_page::{ListOptions, ListView};
use crate::tools::list_page::{ListKind, ListObservation, ListScope};
use crate::{
    models::review_comment::storage_id,
    server::GitBucketMcpServer,
    tools::{
        response::{from_gb_error, read_only_error, success, validation_error, ToolResult},
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
    #[schemars(
        description = "Maximum returned items: 1..100, default 50; byte limit may return fewer"
    )]
    pub limit: Option<u64>,
    #[schemars(
        description = "Server-issued continuation; reuse identical filters, view and limit before expiry"
    )]
    pub cursor: Option<String>,
    #[schemars(
        description = "summary (default) omits/previews fields explicitly; full returns complete model fields within byte limits"
    )]
    pub view: Option<ListView>,
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddPullRequestReviewCommentParams {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub body: String,
    #[schemars(
        description = "Repository-relative Git path, preserved exactly; no leading/trailing whitespace"
    )]
    pub path: String,
    #[schemars(
        description = "Full lowercase SHA-1 of the PR's current head; historical commits are unsupported"
    )]
    pub commit: String,
    #[schemars(description = "Positive old-side deleted/changed line; supply exactly one side")]
    pub old_line: Option<u64>,
    #[schemars(description = "Positive new-side added/changed line; supply exactly one side")]
    pub new_line: Option<u64>,
    #[schemars(
        description = "Required explicit guard: preflight accepts a race window; atomic is unsupported by Web and rejected before HTTP"
    )]
    pub head_guard: crate::models::review_post::HeadGuard,
}

impl From<AddPullRequestReviewCommentParams> for crate::models::review_post::CreateReviewComment {
    fn from(p: AddPullRequestReviewCommentParams) -> Self {
        Self {
            body: p.body,
            path: p.path,
            commit: p.commit,
            old_line: p.old_line,
            new_line: p.new_line,
            head_guard: p.head_guard,
        }
    }
}

#[tool_router(router = tool_router_review_comment, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "Post one review comment to a changed line in the open PR's current head diff. Requires standard profile and explicit head_guard=preflight accepting its race window. Web cannot preserve outer whitespace or provide atomic guards. Verifies own raw body, commit and PR observations; never retries or falls back to ordinary comments.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn add_pull_request_review_comment(
        &self,
        Parameters(params): Parameters<AddPullRequestReviewCommentParams>,
    ) -> ToolResult {
        if self.is_read_only() {
            return read_only_error();
        }
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(fields) => fields,
            Err(error) => return validation_error(error),
        };
        let number = params.pull_number;
        if number == 0 {
            return validation_error("pull_number must be positive");
        }
        let change = params.into();
        if let Err(error) = crate::models::review_post::CreateReviewComment::validate(&change) {
            return from_gb_error(error);
        }
        match self
            .client
            .add_pull_request_review_comment(&owner, &repo, number, &change)
            .await
        {
            Ok(result) => success(&result),
            Err(error) => from_gb_error(error),
        }
    }
    #[tool(
        description = "Read commit/file/line comments observed on a PR Web conversation page. Summary by default; full view includes observed HTML. Coverage remains partial, without complete stored history or raw Markdown. Requires standard profile and matching PAT/Web credentials. Paged response (default 50, max 100) with has_more/next_cursor/range; continue the same observation before expiry",
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
        self.list_response(
            ListScope::new(ListKind::ReviewComments, &owner, Some(&repo))
                .numbered(params.pull_number),
            ListOptions {
                limit: params.limit,
                cursor: params.cursor,
                view: params.view,
            },
            async {
                let observed = self
                    .client
                    .list_pull_request_review_comments(&owner, &repo, params.pull_number)
                    .await?;
                ListObservation::new(observed.comments, Some(observed.coverage))
            },
        )
        .await
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
