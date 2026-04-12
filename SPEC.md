# GitBucket MCP Server Specification

## 1. Purpose

This document defines the functional specification for `gitbucket-mcp-server`, a Rust-based Model Context Protocol (MCP) server for GitBucket.

The server provides AI clients with a stable tool interface for common GitBucket operations against repositories, issues, pull requests, and users. `PLAN.md` describes implementation approach and roadmap; this document defines expected behavior.

## 2. Scope

Supported capabilities:

- Repository lookup, creation, forking, branch listing, and label management
- Issue lookup, creation, update, comment listing, and comment creation
- Pull request lookup, creation, merge, and comment creation
- Authenticated user lookup and username-based user lookup

Out of scope:

- Repository deletion
- Streamable HTTP or SSE transport
- Full MCP end-to-end integration guarantees beyond stdio transport

## 3. Runtime Model

- The server runs as a stdio MCP server.
- `stdout` is reserved for MCP protocol traffic.
- Logs must be written to `stderr`.
- The server advertises MCP tool capability and exposes the implementation name `gitbucket-mcp-server`.

## 4. Configuration

Configuration sources:

1. Environment variables
2. TOML config file

Environment variables override file values.

Supported variables:

- `GITBUCKET_URL`
- `GITBUCKET_TOKEN`
- `GITBUCKET_USERNAME`
- `GITBUCKET_PASSWORD`
- `GITBUCKET_MCP_CONFIG_DIR`

Default config path:

```text
~/.config/gitbucket-mcp-server/config.toml
```

Config file format:

```toml
url = "https://gitbucket.example.com"
token = "your-personal-access-token"
```

Requirements:

- `GITBUCKET_URL` and `GITBUCKET_TOKEN` are required unless supplied by config file.
- `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are optional, but must be supplied together when used.
- `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are environment-variable only; `config.toml` must not contain `username` or `password`.
- Empty values are invalid.
- Missing config files are treated as “not configured yet”.
- Malformed or unreadable config files must fail startup with a configuration error.
- On Unix, written config files must use `0600` permissions.

## 5. GitBucket API Contract

- Base URLs are normalized to include scheme and `/api/v3`.
- Authentication uses the `Authorization: token <token>` header.
- Requests accept JSON responses.
- Repository listing first tries `/users/{owner}/repos` and falls back to `/orgs/{owner}/repos` on HTTP 404.
- List endpoints auto-paginate with `page` and `per_page=100` until the final short page.
- `update_label(new_name/color)` may fall back to a GitBucket web session only when the REST `PATCH` endpoint returns HTTP 404 and the target label still exists via `GET`. The web fallback does not support label description updates.
- `update_issue(state=...)` may fall back to a GitBucket web session only when the REST `PATCH` endpoint returns HTTP 404, the target Issue still exists via `GET`, and optional web credentials are configured.
- milestone create, update, and delete may fall back to the GitBucket web UI when the REST endpoint returns HTTP 404 and the target repository or milestone can still be verified.

## 6. MCP Tool Contract

All tools return MCP tool results.

- Success responses use structured JSON content.
- Failures use MCP error tool results with `is_error=true` and a structured payload.

### 6.1 Repository Tools

- `list_repositories(owner)`
- `get_repository(owner, repo)`
- `create_repository(name, description?, private?, auto_init?)`
- `fork_repository(owner, repo)`
- `list_branches(owner, repo)`

### 6.2 Label Tools

- `list_labels(owner, repo)`
- `get_label(owner, repo, name)`
- `create_label(owner, repo, name, color, description?)`
- `update_label(owner, repo, name, new_name?, color?, description?)`
- `delete_label(owner, repo, name)`

### 6.3 Issue Tools

- `list_issues(owner, repo, state?)`
- `get_issue(owner, repo, issue_number)`
- `create_issue(owner, repo, title, body?, labels?, assignees?)`
- `update_issue(owner, repo, issue_number, state?, title?, body?)`
- `list_issue_comments(owner, repo, issue_number)`
- `add_issue_comment(owner, repo, issue_number, body)`

### 6.4 Milestone Tools

- `list_milestones(owner, repo, state?)`
- `get_milestone(owner, repo, milestone_number)`
- `create_milestone(owner, repo, title, description?, due_on?)`
- `update_milestone(owner, repo, milestone_number, title?, description?, due_on?, state?)`
- `delete_milestone(owner, repo, milestone_number)`

### 6.5 Pull Request Tools

- `list_pull_requests(owner, repo, state?)`
- `get_pull_request(owner, repo, pull_number)`
- `create_pull_request(owner, repo, title, head, base, body?)`
- `merge_pull_request(owner, repo, pull_number, commit_message?)`
- `add_pull_request_comment(owner, repo, pull_number, body)`

### 6.6 User Tools

- `get_authenticated_user()`
- `get_user(username)`

## 7. Input Validation Rules

- Required string fields must not be blank after trimming.
- `create_label.color` and `update_label.color` must be a 6-digit hex value and may optionally include a leading `#`.
- `list_issues.state`, `list_milestones.state`, and `list_pull_requests.state` must be one of `open`, `closed`, or `all`.
- `update_issue.state` and `update_milestone.state` must be one of `open` or `closed`.
- `update_issue` must receive at least one of `state`, `title`, or `body`.
- `update_label` must receive at least one of `new_name`, `color`, or `description`.
- `update_milestone` must receive at least one of `title`, `description`, `due_on`, or `state`.
- On GitBucket instances without REST issue update support, `update_issue` may fall back through the web UI for `state`, `title`, and `body` changes when web credentials are configured.
- Optional string fields may be trimmed before sending to GitBucket.
- Validation failures must be returned without issuing an outbound API request.

## 8. Error Handling

Expected error classes:

- Configuration errors
- HTTP transport errors
- GitBucket API errors
- JSON serialization or deserialization errors
- URL parsing errors

Behavior:

- Startup configuration failures stop the process.
- Non-success GitBucket responses are surfaced as API errors with HTTP status and body text when available.
- Tool handlers convert internal errors to structured MCP error payloads.

## 9. Security Requirements

- Tokens must never be hardcoded in repository files.
- Documentation and examples must use placeholders only.
- Config persistence must preserve restricted file permissions where supported.
- Web fallback passwords must not be stored in `config.toml`.

## 10. Future Extensions

The following are intentionally deferred and may be specified later:

- Additional write-path E2E coverage beyond repository creation, Issue flows, and pull request flows
- Additional GitBucket endpoints
- Alternative transports beyond stdio
