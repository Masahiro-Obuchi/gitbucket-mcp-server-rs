use crate::{
    models::{
        issue_label::LabelNames,
        label::Label,
        list_page::{ListOptions, ListView},
    },
    server::GitBucketMcpServer,
    tools::{
        list_page::{ListKind, ListObservation, ListScope},
        response::{from_gb_error, read_only_error, success, validation_error, ToolResult},
        validation::repository_fields,
    },
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListIssueLabelsParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Maximum returned labels: 1..100, default 50; byte limit may return fewer"
    )]
    pub limit: Option<u64>,
    #[schemars(
        description = "Server-issued continuation; reuse identical target, view and limit before expiry"
    )]
    pub cursor: Option<String>,
    #[schemars(
        description = "summary (default) or full, within response and observation byte limits"
    )]
    pub view: Option<ListView>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeIssueLabelsParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Exact distinct label names; unknown names create repository labels with generated colors. Replace with [] to remove all assignments"
    )]
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveIssueLabelParams {
    pub owner: String,
    pub repo: String,
    #[schemars(description = "Issue or PR number, 1..2147483647")]
    pub issue_number: u64,
    #[schemars(
        description = "Exact assigned label name; removes this assignment and keeps the repository label definition"
    )]
    pub name: String,
}

#[derive(Serialize)]
struct IssueLabelsResult {
    issue_number: u64,
    labels: Vec<Label>,
}

#[tool_router(router = tool_router_issue_label, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "List assigned labels on an Issue or PR, after checking that the parent exists. Paged cached observation: summary default, has_more/next_cursor/range; continue with identical arguments",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn list_issue_labels(
        &self,
        Parameters(params): Parameters<ListIssueLabelsParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&params.owner, &params.repo) {
            Ok(value) => value,
            Err(error) => return validation_error(error),
        };
        if let Err(error) = crate::api::issue_label::number(params.issue_number) {
            return from_gb_error(error);
        }
        self.list_response(
            ListScope::new(ListKind::IssueLabels, &owner, Some(&repo))
                .numbered(params.issue_number),
            ListOptions {
                limit: params.limit,
                cursor: params.cursor,
                view: params.view,
            },
            async {
                ListObservation::new(
                    self.client
                        .list_issue_labels(&owner, &repo, params.issue_number)
                        .await?,
                    None,
                )
            },
        )
        .await
    }

    #[tool(
        description = "Add one or more label assignments to an Issue or PR, preserving other labels. Unknown names also create repository labels with generated colors. Names are exact, not trimmed. Verifies the complete resulting set; no automatic retry",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn add_issue_labels(
        &self,
        Parameters(params): Parameters<ChangeIssueLabelsParams>,
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
        let change = LabelNames {
            labels: params.labels,
        };
        if let Err(error) = crate::api::issue_label::names(&change, false) {
            return from_gb_error(error);
        }
        match self
            .client
            .add_issue_labels(&owner, &repo, params.issue_number, &change)
            .await
        {
            Ok(labels) => success(&IssueLabelsResult {
                issue_number: params.issue_number,
                labels,
            }),
            Err(error) => from_gb_error(error),
        }
    }

    #[tool(
        description = "Replace ALL assigned labels on an Issue or PR. Omitted existing assignments are removed; [] clears them all. Unknown names create repository labels. Repository definitions remain. Verifies the full resulting set; no automatic retry",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn replace_issue_labels(
        &self,
        Parameters(params): Parameters<ChangeIssueLabelsParams>,
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
        let change = LabelNames {
            labels: params.labels,
        };
        if let Err(error) = crate::api::issue_label::names(&change, true) {
            return from_gb_error(error);
        }
        match self
            .client
            .replace_issue_labels(&owner, &repo, params.issue_number, &change)
            .await
        {
            Ok(labels) => success(&IssueLabelsResult {
                issue_number: params.issue_number,
                labels,
            }),
            Err(error) => from_gb_error(error),
        }
    }

    #[tool(
        description = "Remove one label assignment from an Issue or PR, preserving all others and the repository label definition. An already absent assignment is a checked no-op. Verifies the complete resulting set",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn remove_issue_label(
        &self,
        Parameters(params): Parameters<RemoveIssueLabelParams>,
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
        if let Err(error) = crate::api::issue_label::name(&params.name) {
            return from_gb_error(error);
        }
        match self
            .client
            .remove_issue_label(&owner, &repo, params.issue_number, &params.name)
            .await
        {
            Ok(labels) => success(&IssueLabelsResult {
                issue_number: params.issue_number,
                labels,
            }),
            Err(error) => from_gb_error(error),
        }
    }
}
