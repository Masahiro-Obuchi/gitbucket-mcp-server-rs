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

Optional web fallback operations require both `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` in the environment. These credentials are only for GitBucket web UI fallback operations, not for Git over HTTP.

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

- `update_label` first tries the REST endpoint. If the endpoint is unavailable, the server can use a web UI fallback for name and color changes.
- The web fallback cannot perform description-only label updates. If a request changes name or color and also includes a description, name and color can still be updated, but the description is not changed by the fallback.
- Some GitBucket issue responses omit `closed_at`. For closed issues, the server fills missing `closed_at` from `updated_at` as a best-effort GitHub-compatible value.

## Safety

Ask for confirmation before:

- creating repositories or forks;
- deleting labels or milestones;
- closing issues or pull requests when the user's intent is ambiguous;
- merging pull requests.

When a user asks for a destructive or irreversible action, restate the target owner, repository, and object number or name before calling the tool.

Prefer exact owner, repository, branch, issue number, and pull request number values from tool results over guesses. Trim accidental whitespace, but do not rewrite names by assumption.
