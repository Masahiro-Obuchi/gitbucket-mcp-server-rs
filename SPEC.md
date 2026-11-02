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
- Config saves use a private temporary file and atomic replacement; Unix files use `0600` even when replacing a permissive file. Symlink/non-regular destinations are rejected.
- Unix loads open without following symlinks, reject non-regular files, and warn about group/other permissions without modifying the file. Parent directories must be trusted. Non-Unix deployments must manage OS ACLs.
- Config/credential Debug output is redacted. TOML parse errors report position and a generic explanation without source text.

## 5. GitBucket API Contract

- Base URLs are normalized to include scheme and `/api/v3`, preserving IPv6 and context paths. Only HTTP(S) hosts are accepted; userinfo, query, fragment, backslashes, dot segments, invalid percent escapes, escaped controls and escaped separators are rejected.
- Owner/username identifiers accept ASCII alphanumeric characters and `-_.`; repository identifiers also accept `+`. Leading hyphens/underscores and dot segments are rejected before HTTP. Length/reserved-name validation remains with GitBucket.
- All REST/web paths are constructed with segment encoding. Labels and branch names are not subject to the owner/repository grammar; dot-only segments and controls are rejected. Low-level Rust string paths must start with `/` and must not embed queries, fragments or percent escapes; query values use query builders.
- REST/web redirects are limited to ten hops within the configured origin and context path, without userinfo or encoded traversal/separators. External proxy redirects require configuring the final URL.
- The API suffix is removed exactly once when defining the web redirect boundary, including contexts ending in `/api/v3`.
- Rust POST queries use `post_with_query` with separate query pairs; embedded query syntax in paths remains rejected.
- Authentication uses the `Authorization: token <token>` header.
- Requests accept JSON responses.
- Repository listing first tries `/users/{owner}/repos` and falls back to `/orgs/{owner}/repos` on HTTP 404.
- Issue and pull request lists use `page` and `per_page=25`, terminating only on
  an empty page. A short page alone does not prove completion.
- Repository, branch, label, milestone, and issue comment lists are fetched once
  without `page`/`per_page`. State filters are retained where applicable.
- Paged issue/PR results must have distinct numbers. Repeated numbers, page
  failures, or the 100-request limit return an error, not a successful partial
  list. The terminal empty-page request counts against this limit.
- Lists are not atomic snapshots of concurrent activity. Existing MCP list
  result field names and state-filter inputs remain unchanged.
- `update_label(new_name/color)` may fall back to a GitBucket web session only when the REST `PATCH` endpoint returns HTTP 404, the target label still exists via `GET`, and optional web credentials are configured. The web fallback does not support label description updates.
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

Milestone creation/update validate due dates before network calls. Inputs accept
`YYYY-MM-DD` (UTC midnight) or RFC3339, normalize offsets to UTC, and use whole
seconds to match GitBucket JSON precision. Invalid dates, leap seconds, and UTC
years outside 0001–9999 are rejected. Fractional seconds are truncated.

Update intent distinguishes omitted values (keep), supplied values (set), and
empty description/due_on strings (clear). Before PATCH, fetch the current
milestone and construct complete title/state/description/due_on values. Explicit
clears are sent as null in the REST payload. Unchanged requests need no write.
Verify the response against requested and preserved values.

The existing 404 web compatibility path remains available, including for titles
that violate the official REST character restrictions. It is not evidence that
the endpoint is absent. Web content forms must preserve UTC-midnight dates and
description whitespace; reject requests that cannot be represented before a
web write. State-only web changes do not submit the content form. Round-trip
date verification also detects differing web-server timezone interpretation.

Failures after a write or during its follow-up read are outcome_unknown;
mismatching returned values are update_not_verified. Creation results must
identify a newly observed ID rather than an existing same-title milestone.
Concurrent-edit atomicity is not guaranteed. HTTP 403/422 rejections do not
trigger fallback, and uncertain responses do not trigger automatic replay.

### 6.5 Pull Request Tools

- `list_pull_requests(owner, repo, state?)`
- `get_pull_request(owner, repo, pull_number)`
- `create_pull_request(owner, repo, title, head, base, body?)`
- `update_pull_request(owner, repo, pull_number, state?, title?, body?, base?)`
- `merge_pull_request(owner, repo, pull_number, commit_message?)`
- `add_pull_request_comment(owner, repo, pull_number, body)`

`update_pull_request` requires at least one change. Omitted title/body values are
preserved by fetching missing content before constructing GitBucket's coupled
title/body update. A body of `""` explicitly clears content; body whitespace is
preserved. State/base-only changes do not resend content. Base branches are
validated before the PATCH. Successful responses must match requested fields
and any content sent to preserve an omitted field.

`update_not_verified` and `outcome_unknown` are structured tool errors with
`is_error=true`. They do not imply that no change was applied, and must not cause
an automatic retry. This workflow does not provide atomic concurrent-edit
protection. The existing 404 compatibility fallback is retained for legacy
instances and its returned PR is verified as well.
Fallback errors retain the operation stage: preparation and authentication
failures retain their original error; failed reads after a web write return
`outcome_unknown` regardless of HTTP status. A later failure in a sequence of
web writes also returns `outcome_unknown` because earlier changes may persist.

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
- Optional string fields may be trimmed before sending to GitBucket, except that
  `update_pull_request.body` preserves the supplied text exactly.
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
- Non-success GitBucket responses, including status wrappers, use the HTTP status and a standard reason. Raw error bodies are not returned or read for HTTP failures. Web failures also omit error bodies. HTTP diagnostics expose a failure category without URLs; JSON diagnostics expose line/column without values. Successful response-size limits and timeout budgets are separate follow-up work.
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
