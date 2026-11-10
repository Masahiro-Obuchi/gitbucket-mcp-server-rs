use crate::{
    models::list_page::{ListOptions, ListView},
    server::GitBucketMcpServer,
    tools::{
        list_page::{ListKind, ListObservation, ListScope},
        response::{validation_error, ToolResult},
        validation::repository_fields,
    },
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPullRequestChangesParams {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub limit: Option<u64>,
    pub cursor: Option<String>,
    pub view: Option<ListView>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPullRequestDiffParams {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    #[schemars(
        description = "Exact old_path or new_path from list_pull_request_files; omit for all blocks. Whitespace is preserved; ambiguous paths fail."
    )]
    pub path: Option<String>,
    #[schemars(
        description = "Maximum 4096-byte UTF-8 chunks, 1..100, default 50; response byte limit can return fewer"
    )]
    pub limit: Option<u64>,
    #[schemars(
        description = "Server-issued continuation; reuse identical target/path/limit within expiry"
    )]
    pub cursor: Option<String>,
}
#[tool_router(router=tool_router_pull_request_changes,vis="pub")]
impl GitBucketMcpServer {
    #[tool(
        description = "List commits in the stored PR base/head range with one unpaged REST request and before/after metadata checks. PAT only. Summary previews long messages explicitly; full preserves fields within page limits. Continue the cached observation via has_more/next_cursor/range.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn list_pull_request_commits(
        &self,
        Parameters(p): Parameters<ListPullRequestChangesParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&p.owner, &p.repo) {
            Ok(v) => v,
            Err(e) => return validation_error(e),
        };
        if p.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        self.list_response(
            ListScope::new(ListKind::PullRequestCommits, &owner, Some(&repo))
                .numbered(p.pull_number),
            ListOptions {
                limit: p.limit,
                cursor: p.cursor,
                view: p.view,
            },
            async {
                let d = self
                    .client
                    .list_pull_request_commits(&owner, &repo, p.pull_number)
                    .await?;
                ListObservation::new(d.commits, None)?.with_metadata("comparison", &d.comparison)
            },
        )
        .await
    }
    #[tool(
        description = "List exact old/new paths, statuses, modes and counts from the full bounded stored-base/head Web patch. Requires standard and matching PAT/Web credentials. Binary payloads are unavailable; upstream may split renames into delete/add. Returns paged files, comparison and has_more/next_cursor/range.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn list_pull_request_files(
        &self,
        Parameters(p): Parameters<ListPullRequestChangesParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&p.owner, &p.repo) {
            Ok(v) => v,
            Err(e) => return validation_error(e),
        };
        if p.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        self.list_response(
            ListScope::new(ListKind::PullRequestFiles, &owner, Some(&repo)).numbered(p.pull_number),
            ListOptions {
                limit: p.limit,
                cursor: p.cursor,
                view: p.view,
            },
            async {
                let d = self
                    .client
                    .list_pull_request_files(&owner, &repo, p.pull_number)
                    .await?;
                ListObservation::new(d.files, None)?.with_metadata("comparison", &d.comparison)
            },
        )
        .await
    }
    #[tool(
        description = "Read exact upstream patch text as UTF-8 chunks with per-file byte ranges. Requires standard and matching PAT/Web credentials. Optional exact old/new path selects one block. Concatenate chunks per file and follow has_more/next_cursor; each page can be incomplete. Binary markers have no binary payload. Stored base/head checks are not atomic or a fresh base-branch comparison.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn get_pull_request_diff(
        &self,
        Parameters(p): Parameters<GetPullRequestDiffParams>,
    ) -> ToolResult {
        let (owner, repo) = match repository_fields(&p.owner, &p.repo) {
            Ok(v) => v,
            Err(e) => return validation_error(e),
        };
        if p.pull_number == 0 {
            return validation_error("pull_number must be positive");
        }
        if p.path
            .as_deref()
            .is_some_and(|v| v.is_empty() || v.contains('\0'))
        {
            return validation_error("path must be a nonempty exact Git path without NUL");
        }
        self.list_response(ListScope::new(ListKind::PullRequestDiff,&owner,Some(&repo)).numbered(p.pull_number).path(p.path.as_deref()),
            ListOptions{limit:p.limit,cursor:p.cursor,view:Some(ListView::Full)},async{
                let d=self.client.get_pull_request_diff(&owner,&repo,p.pull_number,p.path.as_deref()).await?;
                let selection=serde_json::json!({"selected_path":d.selected_path,"total_patch_bytes":d.total_patch_bytes,
                    "selected_patch_bytes":d.selected_patch_bytes,"total_files":d.total_files,"selected_files":d.selected_files,
                    "chunk_byte_limit":4096,"byte_offsets":"per_file_upstream_patch","binary_payloads_available":false});
                ListObservation::new(d.chunks,None)?.with_metadata("comparison",&d.comparison)?.with_metadata("diff",&selection)
            }).await
    }
}
