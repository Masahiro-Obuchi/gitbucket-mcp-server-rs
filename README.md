# GitBucket MCP Server
[![CI](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/ci.yml)
[![E2E](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/e2e.yml/badge.svg)](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/e2e.yml)
[![Release](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/release.yml/badge.svg)](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/release.yml)

A [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server for [GitBucket](https://gitbucket.github.io/), written in Rust.

This server enables AI assistants (Claude Desktop, GitHub Copilot, etc.) to interact with GitBucket repositories, labels, milestones, issues, and pull requests through the MCP protocol.
This is an unofficial community project and is not affiliated with the GitBucket project.

## Features

- **Repository Management**: List, view, create, fork repositories and list branches
- **Labels**: List, view, create, update, and delete repository labels
- **Milestones**: List, view, create, update, and delete repository milestones
- **Issue Tracking**: List, view, create, update issues; manage comments
- **Pull Requests**: List, view, create, update, merge PRs; add comments
- **User Info**: Get authenticated user and look up other users

## Requirements

- Rust 1.70+
- A GitBucket instance with a Personal Access Token

## Installation

### GitHub Releases

Tagged releases publish prebuilt archives for:

- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Download the archive that matches your platform from GitHub Releases, extract it, and place `gitbucket-mcp-server` on your `PATH`.

Typical install locations:

- Linux/macOS: `~/.local/bin/gitbucket-mcp-server` or another directory already on `PATH`
- Windows: a directory on `PATH`, such as `%USERPROFILE%\bin\gitbucket-mcp-server.exe`

Archive names follow this pattern:

```text
gitbucket-mcp-server-<version>-<target>.tar.gz
gitbucket-mcp-server-<version>-<target>.zip
```

Each release also includes a `.sha256` checksum file.

For maintainer release steps, see [RELEASE.md](./RELEASE.md).

### cargo install

Install the latest published version from crates.io:

```bash
cargo install gitbucket-mcp-server --locked
```

To install directly from Git:

```bash
cargo install --git https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs --locked
```

To install a tagged release:

```bash
cargo install --git https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs --tag v0.1.0 --locked
```

`cargo install` places the binary in `$CARGO_HOME/bin`, which is usually `~/.cargo/bin` on Linux/macOS and `%CARGO_HOME%\bin` on Windows (by default `%USERPROFILE%\.cargo\bin`).

### From source

```bash
git clone https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs.git
cd gitbucket-mcp-server-rs
cargo build --release
```

The binary will be at `target/release/gitbucket-mcp-server`.
If you want to use it directly from your shell or MCP client config, copy it to a directory on `PATH`, for example `~/.local/bin/`.

## Configuration

Configuration can be provided via a **TOML config file** and/or **environment variables**. Environment variables take priority over the config file.

### Config File

Create `~/.config/gitbucket-mcp-server/config.toml`:

```toml
url = "https://gitbucket.example.com"
token = "your-personal-access-token"
```

The server does not create a config file on startup. Before editing it on Unix, create the directory and file privately:

```bash
(umask 077; mkdir -p ~/.config/gitbucket-mcp-server; touch ~/.config/gitbucket-mcp-server/config.toml)
chmod 600 ~/.config/gitbucket-mcp-server/config.toml
```

The library's save method atomically replaces regular files with a `0600` file, including existing `0644` files. On Unix, loading rejects symlinks and non-regular files, and warns about group/other permissions without changing the file. Save rejects symlink destinations. Keep parent directories under your control. On non-Unix systems, restrict access using the OS's ACL controls; Unix mode bits are not an ACL guarantee. Web fallback credentials are intentionally **not** read from `config.toml`; set `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` via environment variables only.

The config directory can be overridden with the `GITBUCKET_MCP_CONFIG_DIR` environment variable.

GitBucket URLs accept HTTP(S), a host (including IPv6), an optional port and a context path. A missing scheme defaults to HTTPS. Embedded credentials, query strings, fragments, dot segments, backslashes, and escaped path separators are rejected rather than silently normalized to another target. Put the PAT in `GITBUCKET_TOKEN`. REST and web redirects must stay within the configured origin and context path; configure the final instance URL if a proxy redirects to a different host or context.

Owner/user names accept ASCII letters, digits, `-`, `_` and `.`; repository names also accept `+`. A leading `-` or `_`, and the complete names `.` and `..`, are rejected. These follow the identifier character rules in GitBucket 4.44.0/4.48.0; GitBucket may impose additional length or reserved-name restrictions. Label and branch names use their own segment encoding, so legitimate spaces, Unicode and branch slashes do not become URL syntax.

Low-level Rust POST calls with query parameters use `post_with_query(path, query, body)`.
The path remains free of query syntax; query keys and values are encoded separately.

Debug output redacts configuration and credentials. API errors report the HTTP status and a standard reason rather than the server's raw error body, which can echo credentials or private content. HTTP diagnostics omit request URLs and JSON decoding diagnostics omit values, including when traversing the standard error source chain. Read/preparation error kinds are retained. Write failures include operation outcomes and a structured cause as described below.

### Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `GITBUCKET_URL` | ✅* | GitBucket instance URL | `https://gitbucket.example.com` |
| `GITBUCKET_TOKEN` | ✅* | Personal Access Token | `abc123...` |
| `GITBUCKET_USERNAME` | ❌ | GitBucket username for web fallback operations | `alice` |
| `GITBUCKET_PASSWORD` | ❌ | GitBucket password for web fallback operations | `secret-pass` |
| `GITBUCKET_API_PROFILE` | ❌ | Upstream API contract; default `rest-only` | `standard` |
| `GITBUCKET_READ_ONLY` | ❌ | Expose and allow only read tools; `true` or `false` (default) | `true` |
| `GITBUCKET_MCP_CONFIG_DIR` | ❌ | Override config directory | `/custom/path` |

\* Required if not set in config file. Environment variables override config file values. `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are optional, but must be set together when used.

`GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are used only when this MCP server
uses a selected web compatibility route. Credentials alone do not enable that route;
select `GITBUCKET_API_PROFILE=standard` as described below. They are **not** used for Git over HTTP operations such as `git clone`,
`git fetch`, or `git push`. Configure your Git credential helper separately if
Git commands prompt for a username or password.

### Read-only access

Set `GITBUCKET_READ_ONLY=true` to register only the 14 list/get tools. The server
also rejects write calls with `read_only_error` before any HTTP or authentication
requests, including calls to a known write name and direct Rust tool handlers.
The setting is env-only; values other than `true` or `false` fail startup.
Normal read/write access is the default. Rust server users opt in with
`.with_access_mode(server::AccessMode::ReadOnly)`; library constructors do not
read this environment variable. This mode controls the MCP server's tools;
standalone `GitBucketClient` calls use the client's own capabilities.

All tools advertise read-only, destructive, idempotent and open-world hints.
Reads and deletions are marked idempotent; other writes conservatively are not,
because updates can emit hooks or notifications. Updates, deletions and merge
are destructive hints; creates/comments/forks are additive hints. Annotations
are client hints, while read-only access is enforced independently by the server.

### GitBucket API profiles

`GITBUCKET_API_PROFILE` selects an upstream operation contract. It has two values:

| Value | Behavior |
| --- | --- |
| `rest-only` (default) | Use REST and return its errors. Do not write through the web UI, even when web credentials are configured. |
| `standard` | Use the common standard GitBucket contract verified on 4.44.0 and 4.48.0. Web routes require both web credentials. |

Under `standard`, operations use the following routes before attempting a write:

| Operation | Route |
| --- | --- |
| PR update; label rename; milestone delete | REST |
| Ordinary Issue update | Web; the standard REST Issue PATCH is unavailable |
| Label same-name color change | Web; the standard REST implementation rejects its own existing name with 422 |
| Milestone create/update with REST-valid title | REST |
| Milestone create/update with a title outside `[a-zA-Z0-9+_.-]+` | Web, subject to the date/whitespace safeguards below |

No profile performs fallback writes after a REST error. In particular, a 404 can mean invalid input, a hidden resource or missing permission; it does not prove that an endpoint is absent. The old PR → Issue → Web, label and milestone error-driven write paths have been removed. Read-only repository lookup can still try the organization endpoint after a user lookup returns 404.

The profile is selected explicitly, not detected from a version string or failed write. Plugins/custom builds may differ; use `rest-only` to try their REST implementations. For standard web operations, set `GITBUCKET_API_PROFILE=standard` and both `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD`. The profile setting is env-only, not a TOML field; URL/token precedence is unchanged. Rust callers use `.with_api_profile(ApiProfile::Standard)`.

Label create/update inputs contain only supported fields. `description` is absent from their schema and Rust request types. Supplying it, even as null or combined with name/color changes, returns an invalid-parameters error before HTTP. Other unknown label-write arguments are rejected as well.

Before web sign-in, the PAT's `/api/v3/user` login must equal `GITBUCKET_USERNAME`. After sign-in, the server calls the same endpoint using only the web cookie and requires the same login; it repeats that cookie check before each resource write. PAT headers are never added to web requests. An expired or different session stops the operation. There is no impersonation option. Password-based GitBucket sign-in is required for web routes; SSO/MFA challenges and account-setup redirects are unsupported. REST-only operations continue to use the PAT.

Web requests do not automatically follow redirects. Sign-in follows recognized landing pages using GET; each resource write follows only its operation's known read URL after 301/302/303. The server rejects 307/308 without replaying credentials or form bodies. Jetty's URL session IDs are removed from recognized redirects; authentication uses cookies. Authentication forms, validation errors and unexpected response shapes are errors, even with HTTP 200. Label lookup uses an HTML parser and exact decoded label names within repository-specific rows.

Authentication form actions are resolved against the response URL and matched at the configured web root. Repository names such as `oauth-tools` and context paths such as `/oidc-context` remain valid. Invalid repository targets are rejected before web identity requests.

Web Issue updates verify requested fields through REST after content writes and at completion, stopping before subsequent fields when verification fails. Label and milestone writes also verify their results; deletion verifies a follow-up REST 404. These reads observe the configured account's view and do not provide atomic concurrency protection. `authentication_error` before the first resource write means authentication could not be established; failures after a write carry a structured operation outcome, including partial progress. Inspect that outcome and the resource before retrying.

The label constraint is visible in the upstream [4.44.0 label controller](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala) and [4.48.0 label controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala); target-name lookup collides with the label being edited. It is handled as a known contract, without inferring a route from an arbitrary 422.

### Write outcomes

A failed write returns `is_error=true` with one of these kinds:

| Kind | Meaning |
| --- | --- |
| `operation_partial` | At least one requested field was verified, while other fields are mismatched, uncertain, rejected or not attempted. |
| `operation_unknown` | An attempted write or its verification could not be completed. |
| `operation_not_verified` | A response/read was available but did not match the requested values, without a verified partial result. This does not prove the write never happened. |
| `operation_not_applied` | The attempted REST write was definitively rejected. The original API error is in `cause`. |

The `operation` object gives the name, REST/Web route, target locators and per-field results. Fields have an `intent` of `requested` or `preserved` and a `status` of `verified`, `unchanged`, `mismatched`, `unknown`, `rejected` or `not_attempted`. `verified` means the value matched an observation during this call; `unchanged` means it matched without a write to that field. Neither proves that this operation caused the value or protects against concurrent edits. Preserved fields matching alone do not make a partial result.

Matching preflight values are retained as `unchanged` if a later field fails or the call is interrupted. If a subsequent write includes one of those fields, its status becomes `unknown` until verified again. An unchanged requested value alone does not make the operation partially applied.

For example, an Issue title verified before a body timeout returns:

```json
{
  "kind": "operation_partial",
  "message": "update_issue Partial; inspect the resource before retrying. Cause: Time budget exceeded during MCP tool execution",
  "operation": {
    "name": "update_issue",
    "status": "partial",
    "route": "web",
    "target": {"resource": "issue", "owner": "owner", "repo": "repo", "number": 1},
    "fields": [
      {"field": "title", "intent": "requested", "status": "verified"},
      {"field": "body", "intent": "requested", "status": "unknown"},
      {"field": "state", "intent": "requested", "status": "not_attempted"}
    ]
  },
  "cause": {"kind": "timeout_error", "message": "Time budget exceeded during MCP tool execution"}
}
```

Built-in writes retain their operation and target on failures, including MCP deadlines/cancellation. Detailed field observations cover Issue/PR updates, label/milestone writes and verified deletion. Other creates, comments and merges can have an empty field list; this does not assert any field verification. A creation ID is supplied only when known. Ambiguous milestone creation returns candidate numbers without inventing a created ID. Comment targets use `parent_number`; an unknown authenticated repository owner is omitted. Unexpected response numbers are recorded separately from the intended target.

Pre-write preparation/authentication errors retain their normal error kinds. Low-level Rust helpers and custom adapters without service progress can omit target/route/fields; they remain conservative about uncertain writes. There are no automatic retries or rollbacks. Inspect partial/unknown/unverified results before deciding what to send next, and correct rejected input/permissions before retrying.

This changes the protocol: `outcome_unknown` and `update_not_verified` are removed, with no aliases. Rust callers use `GbMcpError::OperationFailed { outcome, cause }`; the previous two operation error variants are removed. Successful tool payloads and configuration are unchanged.

### HTTP and tool limits

REST and web sessions share these optional, env-only settings. They are not TOML fields; the URL/token precedence above is unchanged.

| Variable | Default | Applies to |
| --- | --- | --- |
| `GITBUCKET_CONNECT_TIMEOUT_SECS` | `10` | Connection establishment, including TLS |
| `GITBUCKET_REQUEST_TIMEOUT_SECS` | `30` | Each ordinary HTTP request, including body reads and redirects |
| `GITBUCKET_MERGE_TIMEOUT_SECS` | `120` | The PR merge HTTP request |
| `GITBUCKET_TOOL_TIMEOUT_SECS` | `180` | A complete MCP tool call, including pagination, sign-in, fallback and verification |
| `GITBUCKET_JSON_LIMIT_BYTES` | `8388608` (8 MiB) | Each successful REST response before JSON decoding |
| `GITBUCKET_HTML_LIMIT_BYTES` | `4194304` (4 MiB) | Each successful web response, including sign-in |

Time overrides must be integer seconds from 1 to 86400; response limits must be 1 to 268435456 bytes. Invalid settings fail startup without echoing the supplied value. The earliest applicable timeout wins: a shorter tool budget also limits a long merge or many individually valid requests. Read and write requests are not automatically retried.

Bodies are limited while streaming, even without `Content-Length`. API error bodies remain unread and are summarized by status. An oversized read returns `response_too_large`; a timed-out read returns `timeout_error`. A mutation whose response times out, disconnects, cannot be decoded or exceeds the limit returns `operation_unknown` or `operation_partial`, since the server may already have saved it. Definite REST rejections (HTTP 4xx except 408) return `operation_not_applied` with an `api_error` cause and the original status. Low-level Rust HTTP helpers retain API rejections as `GbMcpError::Api`. Inspect the target before retrying an unknown outcome.

REST writes stop at HTTP redirects and return `operation_unknown` with the original 3xx status. This prevents resubmitting a write at a redirect target or treating a redirected read's failure as rejection of an already saved write. REST reads follow safe redirects within the configured GitBucket origin and context path; web writes follow only their known read destinations described above.

MCP cancellation stops further local work but cannot undo a remote write. The server classifies interrupted reads as `cancelled_error` and interrupted writes as `operation_unknown` or `operation_partial`; some MCP clients (including rmcp) discard a cancelled request's later response and surface their own cancellation instead. Inspect any possibly submitted write before retrying in that case as well.

These are per-response byte limits and cooperative async time limits, not a total memory cap or a hard CPU deadline. Synchronous decoding/serialization cannot be preempted. Aggregate list/output limits are separate work; no diff endpoint or separate diff limit is introduced here.

Rust callers can pass `api::http::HttpSettings` to `GitBucketClient::new_with_web_auth_and_http_settings`; other client constructors use defaults and do not read env overrides. Create verified web sessions with `client.web_session().await`; sessions inherit the client's HTTP settings. The independent `GitBucketWebSession::sign_in` and `sign_in_with_http_settings` constructors have been removed. `GitBucketMcpServer::new(client)` uses the client's tool budget. `with_api` uses the default budget; `with_api_and_timeout` accepts a validated explicit budget. Custom API implementations default to conservative write outcomes on interruption. To provide precise tracking, opt into `GitBucketApi::tracks_write_attempts` and wrap business writes with `http::mutation` in the calling task. Direct Rust API calls have per-request limits; the complete tool budget/cancellation applies at MCP dispatch.

### Priority

1. **Environment variables** (highest priority)
2. **TOML config file** (`~/.config/gitbucket-mcp-server/config.toml`)

### Creating a Personal Access Token

1. Log in to your GitBucket instance
2. Go to **Account Settings** → **Personal access tokens**
3. Create a new token with appropriate permissions

## Usage

### Standalone

```bash
# Option 1: Using config file (recommended)
# First, create ~/.config/gitbucket-mcp-server/config.toml with url and token
gitbucket-mcp-server

# Option 2: Using environment variables
export GITBUCKET_URL="https://gitbucket.example.com"
export GITBUCKET_TOKEN="your-token"
export GITBUCKET_API_PROFILE="standard"  # optional, opt into standard web operations
export GITBUCKET_USERNAME="alice"         # required for standard web operations
export GITBUCKET_PASSWORD="secret-pass"   # required for standard web operations; env-only
gitbucket-mcp-server
```

At startup the server prints a short readiness message to `stderr`, for example `gitbucket-mcp-server ready`, while `stdout` remains reserved for MCP protocol traffic.

For a post-install smoke check, see [VERIFICATION.md](./VERIFICATION.md).

### Claude Desktop

Add to your Claude Desktop configuration (`~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "gitbucket": {
      "command": "/path/to/gitbucket-mcp-server",
      "env": {
        "GITBUCKET_URL": "https://gitbucket.example.com",
        "GITBUCKET_TOKEN": "your-token"
      }
    }
  }
}
```

### VS Code / GitHub Copilot

`gitbucket-mcp-server` must already be installed and available on your `PATH`.
Use the button below to add the server configuration to VS Code:

[![Add to VS Code](https://img.shields.io/badge/VS_Code-Add_Server-0098FF?style=flat-square&logo=visualstudiocode&logoColor=white)](https://vscode.dev/redirect/mcp/install?name=gitbucket&inputs=%5B%7B%22id%22%3A%22gitbucket_url%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22GitBucket%20URL%22%7D%2C%7B%22id%22%3A%22gitbucket_token%22%2C%22type%22%3A%22promptString%22%2C%22description%22%3A%22GitBucket%20Personal%20Access%20Token%22%2C%22password%22%3Atrue%7D%5D&config=%7B%22command%22%3A%22gitbucket-mcp-server%22%2C%22env%22%3A%7B%22GITBUCKET_URL%22%3A%22%24%7Binput%3Agitbucket_url%7D%22%2C%22GITBUCKET_TOKEN%22%3A%22%24%7Binput%3Agitbucket_token%7D%22%7D%7D)

For manual setup, add this to your user-level VS Code MCP configuration. In VS Code,
use **MCP: Open User Configuration** and avoid committing credentials or credential
input bindings to workspace `.vscode/mcp.json`:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "gitbucket_url",
      "description": "GitBucket URL"
    },
    {
      "type": "promptString",
      "id": "gitbucket_token",
      "description": "GitBucket Personal Access Token",
      "password": true
    }
  ],
  "servers": {
    "gitbucket": {
      "command": "gitbucket-mcp-server",
      "env": {
        "GITBUCKET_URL": "${input:gitbucket_url}",
        "GITBUCKET_TOKEN": "${input:gitbucket_token}"
      }
    }
  }
}
```

If your GitBucket version requires web fallback for operations that are not
available through the REST API, replace the previous example with the
following full user-level MCP configuration, which adds the required VS Code
input variables and environment entries for web fallback credentials:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "gitbucket_url",
      "description": "GitBucket URL"
    },
    {
      "type": "promptString",
      "id": "gitbucket_token",
      "description": "GitBucket Personal Access Token",
      "password": true
    },
    {
      "type": "promptString",
      "id": "gitbucket_api_profile",
      "description": "API profile: rest-only or standard",
      "default": "rest-only"
    },
    {
      "type": "promptString",
      "id": "gitbucket_username",
      "description": "GitBucket username for web fallback"
    },
    {
      "type": "promptString",
      "id": "gitbucket_password",
      "description": "GitBucket password for web fallback",
      "password": true
    }
  ],
  "servers": {
    "gitbucket": {
      "command": "gitbucket-mcp-server",
      "env": {
        "GITBUCKET_URL": "${input:gitbucket_url}",
        "GITBUCKET_TOKEN": "${input:gitbucket_token}",
        "GITBUCKET_API_PROFILE": "${input:gitbucket_api_profile}",
        "GITBUCKET_USERNAME": "${input:gitbucket_username}",
        "GITBUCKET_PASSWORD": "${input:gitbucket_password}"
      }
    }
  }
}
```

Only add `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` when you want web
fallback credentials configured. The standard API profile must also be selected as described above. The credentials must be set together; leaving one blank or setting only
one of them causes startup/configuration errors.

### Codex Skill Sample

This repository includes a sample Codex Skill at [`skills/gitbucket-mcp/`](./skills/gitbucket-mcp/). It gives agents GitBucket-specific guidance for choosing MCP tools, interpreting structured list results, handling compatibility fallbacks, and confirming destructive operations.

## Available Tools

### Repository

| Tool | Description |
|------|-------------|
| `list_repositories` | List repositories for a user or organization |
| `get_repository` | Get repository details |
| `create_repository` | Create a new repository |
| `fork_repository` | Fork a repository |
| `list_branches` | List branches for a repository |

### Issues

| Tool | Description |
|------|-------------|
| `list_issues` | List issues (filterable by state) |
| `get_issue` | Get issue details |
| `create_issue` | Create a new issue |
| `update_issue` | Update issue (state, title, body) |
| `list_issue_comments` | List comments on an issue |
| `add_issue_comment` | Add a comment to an issue |

### Labels

| Tool | Description |
|------|-------------|
| `list_labels` | List labels for a repository |
| `get_label` | Get label details |
| `create_label` | Create a new label |
| `update_label` | Update label name/color via the selected API profile |
| `delete_label` | Delete a label |

### Milestones

| Tool | Description |
|------|-------------|
| `list_milestones` | List milestones for a repository |
| `get_milestone` | Get milestone details |
| `create_milestone` | Create a new milestone |
| `update_milestone` | Update milestone fields and state |
| `delete_milestone` | Delete a milestone |

Milestone updates preserve omitted fields, including the current open/closed
state. The server reads the current milestone and sends the complete values
required by GitBucket's replacement-style PATCH. Use `description: ""` or
`due_on: ""` to clear those fields; omitting them preserves existing values.
State-only updates with REST-valid titles on the tested official versions work
with PAT authentication and do not require web credentials.

`due_on` accepts a valid `YYYY-MM-DD` date or RFC3339 timestamp. Dates mean UTC
midnight; offsets are converted to UTC and fractional seconds are truncated to
GitBucket's whole-second JSON precision. Invalid dates, including invalid leap
days and Unicode input, return `validation_error` before requests are sent.
Leap-second timestamps and years outside 0001–9999 are unsupported.

Official GitBucket REST milestone titles have stricter character restrictions
than the web UI. The matching standard profile routes Japanese or
space-containing titles directly to Web. `rest-only` retains REST errors;
no error status activates a fallback write. The date-only web form is used for content changes
only when it can preserve the date as UTC midnight and preserve description
whitespace; otherwise `unsupported_error` is returned before a web write.
State-only web updates skip the content form and preserve an existing due time.
On a server whose web form interprets dates in a non-UTC timezone, round-trip
verification may fail; inspect the milestone rather than repeating the write.

Create/update responses are verified, and failed post-write reads or mismatched values return
`operation_unknown`, `operation_not_verified` or `operation_partial` with field observations. A web create
must identify a new milestone ID instead of returning a pre-existing same-title
milestone. These are read/modify/write operations, not atomic protection against
concurrent edits. Operation routes follow the API profile above.

### Pull Requests

| Tool | Description |
|------|-------------|
| `list_pull_requests` | List pull requests (filterable by state) |
| `get_pull_request` | Get PR details |
| `create_pull_request` | Create a new pull request |
| `update_pull_request` | Update PR state, title, body, or base branch |
| `merge_pull_request` | Merge a pull request |
| `add_pull_request_comment` | Add a comment to a pull request |

`update_pull_request` preserves fields you omit. GitBucket requires title and body
to be updated together, so changing only one first fetches the PR and preserves
the other. Body text, including Markdown indentation and trailing whitespace, is
sent unchanged; use `body: ""` to clear it. State-only and base-only updates do
not resend title/body. A supplied base branch is checked before updating.

The response is checked against the requested fields and any preserved content.
An unverified update returns `operation_not_verified` or `operation_partial`, with field observations. A failed
response after a write may return `operation_unknown`: inspect the PR before
retrying, because changes may already have been applied. Writes are not
automatically retried. Fetching and updating are separate requests, so this does
not guarantee protection against concurrent edits by other users.

For standard Issue web updates, failures while reading the issue after a write also
return `operation_unknown` or `operation_partial`, including HTTP 403/404 responses. Failures before a
fallback write, such as missing web credentials or sign-in failure, retain their
original error.

### User

| Tool | Description |
|------|-------------|
| `get_authenticated_user` | Get the authenticated user's info |
| `get_user` | Get a user by username |

### Inline review comments

Inline review tools are not implemented yet. Existing PR comments are ordinary
conversation comments. The [P08 design](docs/inline-comment-design.md) records
live contracts for official GitBucket 4.44.0/4.48.0 and the P09/P10 acceptance
conditions. Standard web pages expose rendered HTML with incomplete history;
reading other users' original Markdown requires more than repository read
permission, and the posting form trims content and file paths. The design
separates limited web support from the proposed API extension needed for stronger
guarantees.
Reproduce the research with the [disposable contract probe](TESTING.md#inline-comment-contract-probe-p08).

### Structured Output Shape

MCP structured results are always JSON objects. List tools return their arrays under a stable field name:

| Tool | Result field |
|------|--------------|
| `list_repositories` | `repositories` |
| `list_branches` | `branches` |
| `list_issues` | `issues` |
| `list_issue_comments` | `comments` |
| `list_labels` | `labels` |
| `list_milestones` | `milestones` |
| `list_pull_requests` | `pull_requests` |

All tool inputs reject unknown fields before HTTP with an MCP invalid-parameters
error. In particular, `path`, `line` and `commit_id` do not turn an ordinary PR
comment into an inline comment. Send only fields advertised by the input schema.

Issue/PR bodies, comments, milestone/repository descriptions and merge commit
messages preserve the supplied text, including indentation, trailing spaces and
newlines. Required comments still reject whitespace-only text. Identifiers,
titles, state and dates retain their documented normalization. Empty update
bodies/descriptions explicitly clear content; whitespace-only optional content
is preserved as content, not converted to a clear.

The standard GitBucket Issue web form trims body text. If a changed body has
leading/trailing whitespace, `update_issue` returns `unsupported_error` after its
snapshot read and before any writes, including accompanying title/state changes.
An unchanged body needs no content form, and REST-capable profiles preserve the
text. Milestone web content has the corresponding existing whitespace constraint.

PR results retain `draft`, labels/assignees, `merged_by`, branch users and the
related API URLs supplied by GitBucket. Issue results retain the milestone,
assignee, comment URL and PR link when supplied. Missing metadata arrays and
boolean flags are `null`; explicit empty arrays and `false` remain distinct.
`mergeable: null` means unknown, and a missing repository `fork` flag is also
`null`. URL metadata alone does not establish that the linked API is supported.

List retrieval follows GitBucket's endpoint-specific behavior:

- Issues and pull requests use 25-item pages and continue until an empty page,
  including when a non-final page contains fewer than 25 items.
- Repositories, branches, labels, milestones, and issue comments are fetched once
  without pagination parameters, so responses of 100 or more items are accepted.
- Repeated issue/PR numbers, a failed page, or the existing 100-request pagination
  limit produce a tool error instead of a successful partial list. The limit
  includes the final empty-page request.

List results retain the JSON fields above. They are not atomic snapshots: edits
during retrieval can cause overlapping pages, which are reported as an error;
not every concurrent change can be detected. MCP response-size limits and
client-controlled continuation are separate follow-up work.

Label creation and updates verify the returned name and color. Web updates read the label again; ignored or partly matched changes return `operation_not_verified` or `operation_partial`, and a failed verification read returns an uncertain outcome. Same-value updates return the current label without writing. Label description is not an accepted create/update argument. Any supplied value is rejected by strict input deserialization before HTTP, including an empty or null value or a request that also changes name/color. No requested field is silently dropped. These operations do not provide atomic protection against concurrent edits.

Standard-profile web color changes reject label names containing a comma or leading/trailing whitespace with `unsupported_error` before sign-in. The web form cannot safely preserve those names. The Rust API retains support for whitespace-padded names on REST create/rename operations, and unchanged labels need no web write.

GitBucket's Issue API does not expose `closed_at` on the tested official versions.
Missing closure times remain `null`, even after later edits to a closed Issue;
`updated_at` is never substituted. A closure time supplied by an API is retained.
See [unreleased migration notes](RELEASE.md#unreleased-p07-input-and-model-changes)
for the affected protocol and Rust response types.

## Development

### Build

```bash
cargo build
```

### Test

```bash
# Full test suite (used in CI)
cargo test
```

```bash
# Fast local checks without wiremock-based integration tests
cargo test --lib
cargo test --test mcp_server_test
cargo test --test e2e_test
```

### Lint

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

### CI

GitHub Actions runs the following on every push and pull request:

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

The separate `E2E` workflow is reserved for `workflow_dispatch` and nightly runs. It boots a disposable GitBucket with Docker, exports `GITBUCKET_E2E_*`, runs `cargo test --test e2e_test -- --ignored --nocapture`, and always tears the stack down afterward. The ignored suite covers repository create-path, label create/read/update/delete lifecycle, milestone lifecycle, issue flows, issue web fallback, and pull request write paths.

The `Release` workflow runs on `v*` tags and publishes prebuilt binary archives to GitHub Releases.

## Architecture

```
src/
├── main.rs          # Entry point (stdio transport)
├── lib.rs           # Library root
├── server.rs        # MCP ServerHandler implementation
├── config.rs        # TOML file + environment variable configuration
├── error.rs         # Error types
├── api/             # GitBucket REST API client
│   ├── client.rs    # HTTP client with auth
│   ├── repository.rs
│   ├── milestone.rs
│   ├── issue.rs
│   ├── pull_request.rs
│   └── user.rs
├── models/          # API request/response types
│   ├── user.rs
│   ├── repository.rs
│   ├── milestone.rs
│   ├── issue.rs
│   ├── pull_request.rs
│   └── comment.rs
└── tools/           # MCP tool definitions
    ├── repository.rs
    ├── milestone.rs
    ├── issue.rs
    ├── pull_request.rs
    └── user.rs
```

## Testing Notes

- `tests/api_client_test.rs` uses `wiremock` to validate GitBucket API requests and responses.
- `tests/mcp_server_test.rs` exercises the MCP tool surface over an in-memory transport.
- `tests/e2e_test.rs` provides ignored smoke tests against a real GitBucket instance, including repository creation with branch discovery, label lifecycle coverage, milestone lifecycle coverage, Issue write paths, issue web fallback coverage, and pull request create/comment/merge flows.
- MCP tool calls now return structured success payloads and structured error payloads (`is_error=true`) instead of `"Error: ..."` text conventions.
- `src/tools/*` includes mock-based unit tests for tool validation and success-path behavior.

### Manual E2E Tests

Set the E2E environment variables, then run the ignored test target explicitly:

```bash
export GITBUCKET_E2E_URL="https://gitbucket.example.com/gitbucket"
export GITBUCKET_E2E_TOKEN="your-token"
export GITBUCKET_E2E_OWNER="owner"
export GITBUCKET_E2E_REPO="repo"
export GITBUCKET_E2E_GIT_USERNAME="git-http-username"
export GITBUCKET_E2E_GIT_PASSWORD="git-http-password"
export GITBUCKET_E2E_WEB_USERNAME="gitbucket-username"
export GITBUCKET_E2E_WEB_PASSWORD="gitbucket-password"
export GITBUCKET_E2E_API_PROFILE="standard"
cargo test --test e2e_test -- --ignored --nocapture
```

Optional variables:

- `GITBUCKET_E2E_OWNER`: defaults to the authenticated user for `list_repositories`
- `GITBUCKET_E2E_REPO`: required for milestone, issue, and pull request E2E against an existing repository
- `GITBUCKET_E2E_GIT_USERNAME` / `GITBUCKET_E2E_GIT_PASSWORD`: required for pull request write-path E2E because the tests create and push temporary branches over HTTP
- `GITBUCKET_E2E_WEB_USERNAME` / `GITBUCKET_E2E_WEB_PASSWORD`: optional explicit credentials for web fallback tests; if omitted, E2E reuses the git credentials
- `GITBUCKET_E2E_API_PROFILE`: defaults to `rest-only`; select `standard` with web credentials for the full suite against official GitBucket
- `GITBUCKET_E2E_INSECURE_TLS=true`: allow self-signed or locally trusted HTTPS certificates during E2E runs
- Write-path E2E tests leave created repositories, Issues, comments, pull requests, and merged branches in place; they use unique repo names, branch names, titles, and bodies to avoid collisions across reruns
- Milestone E2E creates, updates, and deletes a unique milestone within the same test so reruns do not accumulate milestone fixtures

### Docker E2E Bootstrap

You can provision a disposable local GitBucket instance for the E2E suite:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
export GITBUCKET_E2E_API_PROFILE=standard
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

The same bootstrap flow is also automated in GitHub Actions through `.github/workflows/e2e.yml`. Use the regular `CI` workflow for fast feedback and the `E2E` workflow for full Docker-backed smoke coverage.

The bootstrap script starts a disposable GitBucket bound to `127.0.0.1`, creates a validation user, issues a personal access token, and provisions the repository. It writes shell-quoted `runtime.env` and a separate `runtime.env.json`, both mode `0600`, in the mode `0700` runtime directory. Both contain test credentials. Source only the generated shell file; CI imports JSON with `python3 scripts/e2e/runtime.py github "$GITHUB_ENV"`, not by stripping `export` from the shell file. Bootstrap removes its temporary cookie/response files on failure. Use `down.sh` to remove the stack and default runtime directory after testing.

A saved E2E token is reused only when `/api/v3/user` identifies the configured
validation user. A token for another account or an unreadable identity response
causes bootstrap to obtain a token for the configured user.

## Acknowledgements

This project exists thanks to [GitBucket](https://gitbucket.github.io/) and the community around it. Thank you for building and maintaining the software that made this MCP server worth creating.

## License

MIT
