---
name: gitbucket-mcp
description: Use this skill when operating a GitBucket instance through the gitbucket-mcp-server MCP tools, including repository, branch, issue, pull request, label, milestone, and user workflows. It explains tool selection, structured result shapes, GitBucket compatibility fallbacks, and safety checks for mutating operations.
---

# GitBucket MCP

Use this skill when the user wants to inspect or modify GitBucket data through `gitbucket-mcp-server`.

## Connection

The MCP server runs over stdio. The client configuration should launch `gitbucket-mcp-server` with either:

- `GITBUCKET_URL` and `GITBUCKET_TOKEN` in the environment, or
- `~/.config/gitbucket-mcp-server/config.toml` containing `url` and `token`.

Web operations require both `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` and `GITBUCKET_API_PROFILE=standard`. The standard upstream contract is verified on 4.44.0 and 4.48.0. The default `rest-only` does not write through Web, even with credentials present. No failed REST write triggers another write route; never infer API absence from a 404. Web credentials are separate from Git-over-HTTP credentials.

Never reveal tokens, passwords, or private instance URLs in responses.

## Tool Groups

Repository tools:

- `list_repositories`
- `get_repository`
- `create_repository`
- `fork_repository`
- `list_branches`

Issue tools:

- `list_issues`
- `get_issue`
- `create_issue`
- `update_issue`
- `list_issue_comments`
- `add_issue_comment`

Pull request tools:

- `list_pull_requests`
- `get_pull_request`
- `create_pull_request`
- `update_pull_request`
- `merge_pull_request`
- `add_pull_request_comment`

Label and milestone tools:

- `list_labels`, `get_label`, `create_label`, `update_label`, `delete_label`
- `list_milestones`, `get_milestone`, `create_milestone`, `update_milestone`, `delete_milestone`

User tools:

- `get_authenticated_user`
- `get_user`

## Result Shapes

MCP structured results are JSON objects. List tools return arrays under stable field names:

| Tool | Field |
|------|-------|
| `list_repositories` | `repositories` |
| `list_branches` | `branches` |
| `list_issues` | `issues` |
| `list_issue_comments` | `comments` |
| `list_labels` | `labels` |
| `list_milestones` | `milestones` |
| `list_pull_requests` | `pull_requests` |

Do not assume list tools return a bare JSON array.

## Common Workflows

For discovery, start with the narrowest read-only tool:

- Unknown owner or identity: call `get_authenticated_user`.
- Unknown repository names: call `list_repositories`.
- Unknown branch names: call `list_branches`.
- Unknown issue or pull request numbers: call `list_issues` or `list_pull_requests`.

For issue work:

1. Use `get_issue` before changing an existing issue when the requested current state is unclear.
2. Use `update_issue` for title, body, or state changes.
3. Use `add_issue_comment` when the user asks to reply without changing issue fields.

For pull request work:

1. Use `get_pull_request` before updating or merging unless the user already gave exact state.
2. Use `update_pull_request` for title, body, base branch, or state changes.
3. Use `merge_pull_request` only after explicit user confirmation.

For labels and milestones:

1. Use `list_labels` or `list_milestones` before creating if duplicate names are possible.
2. Use delete tools only after explicit user confirmation.
3. Treat label colors as six-digit hex values; a leading `#` is accepted by the server.

## GitBucket Compatibility Notes

Some GitBucket versions differ from GitHub-compatible REST behavior.

- Milestone updates preserve fields that are omitted. Pass an empty string to clear `description` or `due_on`; do not send placeholder values for fields that should remain unchanged.
- Milestone dates accept `YYYY-MM-DD` or RFC3339 and normalize to UTC whole seconds. State-only updates use REST when available; web credentials are not generally required for that operation.
- Japanese or space-containing milestone titles select web compatibility under the standard profile because of REST validation restrictions. The web content form cannot safely preserve arbitrary timestamps; `unsupported_error` rejects such requests before a web write.
- Web routes require the PAT, configured username and cookie session to identify the same account; identity is checked before each write. Password sign-in is required; do not try to bypass SSO/MFA challenges or account mismatches. `authentication_error` before the first resource write means no resource mutation was sent.
- For `operation_partial`, `operation_not_verified` or `operation_unknown` errors, inspect the resource before retrying. Read `operation.target`, `route` and each field's status/intent; verified observations can precede later changes, and preserved fields are not newly requested changes. Inspect the nested safe `cause` for the failure reason. `operation_not_applied` identifies a definite REST rejection; fix the cause instead of blindly resending. This includes write timeouts, disconnects, malformed/oversized responses, and interrupted verification; a failed response does not mean the write was not applied. Do the same after client-side cancellation, which may hide the server response.
- Read operations can return `timeout_error`, `cancelled_error` or `response_too_large`. Requests have connection/body deadlines and JSON/HTML byte limits; a complete MCP tool also has a deadline. README documents env-only limit settings. No automatic retry is performed.

- `update_label` uses REST for rename and Web for same-name color changes under a selected standard profile. The default `rest-only` does not write through Web; unknown 404/422 errors do not enable it.
- Label create/update tools do not accept description. Do not send the field, including null or an empty string; unknown arguments are rejected before HTTP, so a combined request cannot partially apply name/color.
- All tools reject unknown arguments before HTTP. Do not send inline `path`, `line` or `commit_id` to ordinary PR comment tools.
- Use `list_pull_request_review_comments` / `get_pull_request_review_comment` for observed commit/file/line comments under the standard profile with matching PAT/Web credentials. IDs use `commit:<ID>` within a repository. They are read tools and work in read-only mode.
- Review list/get bodies are HTML, not original Markdown. Inspect `coverage`: stored history, original positions and stored PR association are incomplete/unverified. A `not_observed` lookup with null comment is not evidence of deletion. `read_conflict` means PR metadata changed during the read; repeat the read if needed.
- Use `add_pull_request_review_comment` for one changed line on an open PR's exact current head: supply `body`, exact repository-relative `path`, full lowercase SHA-1 `commit`, exactly one of `old_line` / `new_line`, and explicit `head_guard: "preflight"` accepting the race window. `atomic` is unsupported. Check the schema and current diff; do not infer positions from rendered read attributes.
- Review posting rejects leading/trailing body/path whitespace and unsupported control characters rather than rewriting them. Internal LF, tabs, code indentation and hard breaks are preserved. Context lines, historical/file-wide comments, whole-file deletion, explicit rename/copy diffs and non-text files are unsupported. Success verifies its own raw body, commit/position and PR-page observation, without claiming stored PR association or complete history. Inspect captured IDs and operation locators after uncertainty or cancellation; never automatically repeat, substitute an ordinary comment, or delete a possibly saved comment.
- Bodies/comments, descriptions and merge messages preserve their original whitespace within their documented Web constraints. Required comments reject whitespace-only text. Empty optional content clears it; spaces are content, not a clear.
- A standard web Issue update cannot preserve leading/trailing whitespace in a changed body. `unsupported_error` stops before any accompanying writes; an unchanged body can be preserved without a content form.
- PR results retain draft, labels/assignees, merged_by and API URLs; Issue results retain milestone and related links. Treat null flags/collections as unknown rather than false/empty. Linked URL metadata does not establish endpoint support.
- Missing Issue `closed_at` remains null. Never substitute `updated_at` or infer a closure time from it.
- `GITBUCKET_READ_ONLY=true` exposes only read tools and rejects writes with `read_only_error`. Tool annotations describe behavior as hints; the server separately enforces this setting.

## Safety

Ask for confirmation before:

- creating repositories or forks;
- deleting labels or milestones;
- closing issues or pull requests when the user's intent is ambiguous;
- merging pull requests.

When a user asks for a destructive or irreversible action, restate the target owner, repository, and object number or name before calling the tool.

Prefer exact owner, repository, branch, issue number, and pull request number values from tool results over guesses. Trim accidental whitespace, but do not rewrite names by assumption.
