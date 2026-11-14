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
- **Pull Requests**: List, view, create, update, merge PRs; read commits/files/diff and add comments
- **User Info**: Get authenticated user and look up other users

## Requirements

- Rust 1.88+ when building from source (prebuilt binaries do not require Rust)
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
cargo build --release --locked
```

The binary will be at `target/release/gitbucket-mcp-server`.
If you want to use it directly from your shell or MCP client config, copy it to a directory on `PATH`, for example `~/.local/bin/`.

The locked dependency graph requires Rust 1.88. CI builds with Rust 1.88.0 and
checks all targets/features on Linux, alongside stable formatting, Clippy and
tests. To reproduce the minimum-version check, see [TESTING.md](./TESTING.md#configuration-cli-and-minimum-rust-p13).

## Configuration

Configuration can be provided via a **TOML config file** and/or **environment variables**. Each environment value takes priority over its file value. When both `GITBUCKET_URL` and `GITBUCKET_TOKEN` are set, the server does not resolve or open the config file, so an unused malformed or unreadable file cannot block startup. With either variable absent, it reads the file to fill only missing values; malformed or unreadable files then fail. Explicit blank/invalid environment values fail instead of falling back. Web credentials are always validated as a pair, including when the file is unused.

### Config File

Create `config.toml` in the platform config directory: `$XDG_CONFIG_HOME/gitbucket-mcp-server` (usually `~/.config/gitbucket-mcp-server`) on Linux, `~/Library/Application Support/gitbucket-mcp-server` on macOS, or `%APPDATA%\gitbucket-mcp-server` on Windows:

```toml
url = "https://gitbucket.example.com"
token = "your-personal-access-token"
```

The server does not create a config file on startup. Before editing it, create
the directory and file privately. For Linux's default path:

```bash
(umask 077; mkdir -p ~/.config/gitbucket-mcp-server; touch ~/.config/gitbucket-mcp-server/config.toml)
chmod 600 ~/.config/gitbucket-mcp-server/config.toml
```

The library's save method atomically replaces regular files with a `0600` file, including existing `0644` files. On Unix, loading rejects symlinks and non-regular files, and warns about group/other permissions without changing the file. Save rejects symlink destinations. Keep parent directories under your control. On non-Unix systems, restrict access using the OS's ACL controls; Unix mode bits are not an ACL guarantee. Web fallback credentials are intentionally **not** read from `config.toml`; set `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` via environment variables only.

The config directory can be overridden with the `GITBUCKET_MCP_CONFIG_DIR` environment variable for local use/tests. An empty override fails when the file is needed; the override is unused when URL and token both come from the environment. Paths use the OS-native representation. URL/token/Web environment values must be valid UTF-8 and nonblank; passwords retain their original bytes rather than being trimmed.

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

Set `GITBUCKET_READ_ONLY=true` to register only the 21 list/get tools. The server
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
| `rest-only` (default) | Use REST and return its errors. Do not use the web UI, even when web credentials are configured. Review-comment reads/posts return `unsupported_error` before HTTP. |
| `standard` | Use the common standard GitBucket contract verified on 4.44.0 and 4.48.0. Web routes require both web credentials. |

Under `standard`, operations select the following routes:

| Operation | Route |
| --- | --- |
| PR update; label rename; milestone delete | REST |
| PR file list/diff | Pinned stored-base/head Web patch; binary payloads unavailable |
| PR review-comment list/get | Web conversation page; HTML and partial coverage only |
| PR review-comment creation | One Web POST after pinned diff/head checks, followed by own raw/commit/PR verification |
| Ordinary Issue update | Web; the standard REST Issue PATCH is unavailable |
| Label same-name color change | Web; the standard REST implementation rejects its own existing name with 422 |
| Milestone create/update with REST-valid title | REST |
| Milestone create/update with a title outside `[a-zA-Z0-9+_.-]+` | Web, subject to the date/whitespace safeguards below |

No profile performs fallback writes after a REST error. In particular, a 404 can mean invalid input, a hidden resource or missing permission; it does not prove that an endpoint is absent. The old PR → Issue → Web, label and milestone error-driven write paths have been removed. Read-only repository lookup can still try the organization endpoint after a user lookup returns 404.

The profile is selected explicitly, not detected from a version string or failed write. Plugins/custom builds may differ; use `rest-only` to try their REST implementations. For standard web operations, set `GITBUCKET_API_PROFILE=standard` and both `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD`. The profile setting is env-only, not a TOML field; URL/token precedence is unchanged. Rust callers use `.with_api_profile(ApiProfile::Standard)`.

Label create/update inputs contain only supported fields. `description` is absent from their schema and Rust request types. Supplying it, even as null or combined with name/color changes, returns an invalid-parameters error before HTTP. Other unknown label-write arguments are rejected as well.

Before web sign-in, the PAT's `/api/v3/user` login must equal `GITBUCKET_USERNAME`. After sign-in, the server calls the same endpoint using only the web cookie and requires the same login; it repeats that cookie check before each resource write. PAT headers are never added to web requests. An expired or different session stops the operation. There is no impersonation option. Password-based GitBucket sign-in is required for web routes; SSO/MFA challenges and account-setup redirects are unsupported. REST-only operations continue to use the PAT.

PAT-only use includes repository/branch/user reads, Issue creation and ordinary
comments, PR creation/update/merge and ordinary comments, label creation/rename/
deletion and REST-valid milestone writes. Under `standard`, only the Web rows
in the table above require the password pair; this includes review-comment
reads. Configuring Web credentials does not cause startup sign-in. `rest-only`
preserves REST errors for operations the stock server does not implement.

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

Built-in writes retain their operation and target on failures, including MCP deadlines/cancellation. Detailed field observations cover supplied Issue creation metadata and explicit PR draft creation, Issue/PR updates, PR merge results, label/milestone writes and verified deletion. Other creates and comments can have an empty field list; this does not assert any field verification. A creation ID is supplied only when known. Ambiguous milestone creation returns candidate numbers without inventing a created ID. Comment creation targets use `parent_number`; item edits/deletes use `number` for the comment ID; an unknown authenticated repository owner is omitted. Unexpected response numbers are recorded separately from the intended target. Merge targets retain the observed head, base branch and captured `result_commit`.

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
2. **TOML config file** (platform config directory, only if URL/token is absent)

### Creating a Personal Access Token

1. Log in to your GitBucket instance
2. Go to **Account Settings** → **Personal access tokens**
3. Create a new token with appropriate permissions

## Usage

### Standalone

```bash
# Option 1: Using config file (recommended)
# First, create config.toml in the platform config directory with url and token
gitbucket-mcp-server

# Option 2: Using environment variables
export GITBUCKET_URL="https://gitbucket.example.com"
export GITBUCKET_TOKEN="your-token"
export GITBUCKET_API_PROFILE="standard"  # optional, opt into standard web operations
export GITBUCKET_USERNAME="alice"         # required for standard web operations
export GITBUCKET_PASSWORD="secret-pass"   # required for standard web operations; env-only
gitbucket-mcp-server
```

With no arguments, the server prints `waiting for MCP initialization` to
`stderr` and waits for an MCP client on stdin/stdout. It reports
`gitbucket-mcp-server ready (MCP initialized)` only after the client's
`initialize` request and `notifications/initialized` notification. This confirms
the protocol handshake; GitBucket connectivity is checked on explicit request
or during tool calls. In server mode, stdout contains only MCP protocol traffic.

### CLI diagnostics

```bash
gitbucket-mcp-server --help
gitbucket-mcp-server --version
gitbucket-mcp-server --check-config
gitbucket-mcp-server --check-connection
```

Help/version work without configuration, logging setup or a Tokio runtime.
`--check-config` validates local configuration, URL/header construction, HTTP
limits, API profile and access mode without HTTP. `--check-connection` additionally
performs only an authenticated REST `GET /api/v3/user`; it rejects an empty login
or a mismatch with a configured Web username. It never submits the Web password
or proves Web-session authentication, repository permissions or write access.
Diagnostic success is one JSON object on stdout; errors go to stderr. Neither
prints credential values, user identity or private instance URLs. Diagnostic
logging ignores `RUST_LOG` to keep dependency traces out of these commands.
Exit codes are 0 for success, 1 for configuration/runtime/connection errors, and
2 for unknown or combined commands. Run diagnostics separately from an MCP client.

### HTTPS and custom certificate authorities

REST, Web sessions and connection diagnostics all use Rustls with native
certificate roots. Install a private CA in the OS trust store used by the server
process, or set `SSL_CERT_FILE` to a PEM CA bundle and/or `SSL_CERT_DIR` to a
directory of CA certificates in that process's environment. These are Rustls
native-root overrides: when either is set, those specified sources replace the
platform root lookup. Include every CA you need, including public roots if
applicable. The certificate must still match the configured hostname and be
valid; configuring a CA does not disable verification.

There is no production CLI/env option to skip TLS verification.
`GITBUCKET_E2E_INSECURE_TLS` belongs only to the live test harness. For a local
trusted-CA example and trust-failure regression, see
[TESTING.md](./TESTING.md#configuration-cli-and-minimum-rust-p13).

For a post-install smoke check, see [VERIFICATION.md](./VERIFICATION.md).
For PR/nightly/release gates, the tested version matrix and deployment acceptance,
see [docs/acceptance.md](./docs/acceptance.md).

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
| `create_issue` | Create an issue with optional assignees, labels and milestone |
| `update_issue` | Update issue (state, title, body) |
| `list_issue_comments` | List comments on an issue |
| `add_issue_comment` | Add a comment to an issue |
| `get_issue_comment` | Get an ordinary Issue/PR conversation comment by comment ID |
| `update_issue_comment` | Replace its full body and verify the stored text and author |
| `delete_issue_comment` | Delete it and verify a subsequent 404 |

### Issue/PR creation metadata

`create_issue(..., milestone?)` accepts an existing repository milestone number
(1..2147483647), in addition to `assignees?` and `labels?`. Supplied references are
checked before POST. Assignee identifiers are trimmed and must be distinct;
label names are exact and must already exist. Issue creation does not create
unknown label definitions as `add_issue_labels` does. Existing closed milestones
can also be selected on the verified versions.

`create_pull_request(..., draft?)` accepts `true` or `false`; omission uses
GitBucket's default ready state. Explicit draft state is checked by a subsequent
GET. Existing get/list responses retain `draft`, including explicit false, and
missing observations remain unknown. PR creation does not accept assignees,
milestone or maintainer_can_modify because the verified controller does not
apply them. Creation fields are not accepted by the update tools in this unit.

These operations use PAT-only REST in both profiles. Supplied Issue creation
metadata is verified by a GET after one POST. Nonempty assignments/milestone need
developer permission: **GitBucket can create the Issue while ignoring these
fields for other readable users.** A verification error may therefore leave an
Issue/PR, and retains its captured number in `operation.target.number`. Inspect
that resource before another write. There is no retry, follow-up mutation or
rollback. Explicit empty arrays are checked as empty, separately from omitted
or unobserved arrays. See [the creation contract](docs/creation-metadata.md).

### Labels

| Tool | Description |
|------|-------------|
| `list_labels` | List labels for a repository |
| `get_label` | Get label details |
| `create_label` | Create a new label |
| `update_label` | Update label name/color via the selected API profile |
| `delete_label` | Delete a label |

### Issue/PR label assignments

| Tool | Description |
| --- | --- |
| `list_issue_labels` | List assigned labels with cached continuation |
| `add_issue_labels` | Add missing assignments and keep all other assignments |
| `remove_issue_label` | Remove one assignment; keep its repository definition |
| `replace_issue_labels` | Replace the entire set; `labels: []` clears it |

All four accept `owner`, `repo` and `issue_number` (also a PR number). Writes
require distinct exact `labels` names, or one `name` for removal. Names are not
trimmed. Unknown names passed to add/replace **create repository labels** with
GitBucket-generated colors. Replacement removes every existing assignment not
listed; repository definitions remain available. Use `list_labels` / `get_label`
for repository definitions and `list_issue_labels` for assignments.

These PAT-only REST tools work in both profiles. Before a change, the parent and
current assignments are read; after a write, the complete resulting set is
verified. Add sends only missing names to avoid GitBucket duplicate insertion
errors. An already matching state is a checked no-op requiring read access;
actual changes require developer permission. Read-only MCP mode blocks every
write call, including no-ops. Write success returns `{issue_number, labels}`
with complete observed label models; list results use the normal bounded pages.
A write or verification failure may follow partial changes or repository-label
creation. No retry, fallback or rollback is performed. See the
[assignment contract](docs/issue-labels.md) for limits and concurrent changes.

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

PR commits/files/diff are documented in [the change-read contract](docs/pull-request-changes.md). Commits use PAT-only REST. Files/diff require the standard profile and matching Web credentials; binary bytes are unavailable and rename detection follows upstream headers. Diff chunks use the existing response/cache bounds and can be continued without refetching the PR.

| Tool | Description |
|------|-------------|
| `list_pull_requests` | List pull requests (filterable by state) |
| `list_pull_request_commits` | Read the stored PR commit range using PAT-only REST |
| `list_pull_request_files` | Read file metadata from the standard Web patch |
| `get_pull_request_diff` | Read exact patch text through bounded UTF-8 chunk continuations |
| `get_pull_request` | Get PR details |
| `create_pull_request` | Create a new pull request |
| `update_pull_request` | Update PR state, title, body, or base branch |
| `merge_pull_request` | Merge a pull request |
| `add_pull_request_comment` | Add a comment to a pull request |
| `list_pull_request_review_comments` | Read observed commit/file/line comments with partial coverage |
| `get_pull_request_review_comment` | Look up `commit:<ID>` within the PR page observation |
| `add_pull_request_review_comment` | Post and verify one changed-line comment at the current PR head |

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

`merge_pull_request` accepts optional `merge_method`: `merge` (default), `squash`
or `rebase`; native repository policy remains enforced by GitBucket. The normal
`head_guard: "none"` default has no reviewed-head constraint. To compare a
reviewed commit, explicitly send `head_guard: "preflight"` with its full lowercase
SHA-1 as `expected_head_sha`. A race window remains; `atomic` is unsupported and
rejected before HTTP. The native API ignores `sha`, so that field is not sent.

Draft/unknown draft and explicitly unmergeable PRs are rejected before a write;
null mergeability stays unknown. Rebase rejects `commit_message` because the
server ignores it. Merge/squash send the original message text, with GitBucket's
native prefix; the REST commit route used here does not expose the full message.

After one PUT, the service checks closed/merged PR state, the result commit's
parents and the base tip. Success returns canonical `sha`, `merged: true`,
`requested_merge_method` and scoped `verification`. Squash/rebase do not claim
that the expected original head is provably in the resulting history. Any failed
or interrupted verification retains known IDs and observations without replay.
See [the merge contract](docs/pull-request-merges.md) and
[migration notes](RELEASE.md#unreleased-p12-verified-pull-request-merges).

### User

| Tool | Description |
|------|-------------|
| `get_authenticated_user` | Get the authenticated user's info |
| `get_user` | Get a user by username |

### Ordinary conversation comments

`get_issue_comment(owner, repo, comment_id)`, `update_issue_comment(..., body)`
and `delete_issue_comment(...)` operate on ordinary Issue **and PR** conversation
comments, using the repository-scoped numeric ID returned by comment creation or
`list_issue_comments`. IDs must be 1..2147483647. They use PAT-only REST under
both profiles. Editing replaces the complete nonblank Markdown body without
trimming. Editing first checks that the ID is a readable conversation comment,
then GETs the stored comment after PATCH to verify the exact text and return the
original author. GitBucket permits the author or a repository developer to edit
or delete. Deletion checks that the comment exists before sending one DELETE, then requires
a subsequent comment GET returning 404 and a successful repository-visibility
read. This handles an upstream bug that emits the deleted ID as HTTP status.
Success is `{deleted: true, comment_id: ...}`. Verification is
sequential and does not prevent concurrent edits or permission changes. Failed
verification after a write may mean the change was saved; do not retry blindly.
See [the ordinary comment contract](docs/ordinary-comments.md).

### Inline review comments

`list_pull_request_review_comments(owner, repo, pull_number)` returns `comments`
and `coverage`. `get_pull_request_review_comment` additionally requires a
repository-scoped `comment_id`, such as `commit:123`, and returns `status`,
`comment` and `coverage`. These read tools require `GITBUCKET_API_PROFILE=standard`
and matching PAT/Web credentials. They work with repository read permission and
are available in read-only mode. They never call the edit-only raw-text endpoint.

Get and full-view list bodies are rendered `body_html`, with `body_format: "html"`
and `body_raw: null`; default summary lists explicitly omit bodies.
Original positions and unavailable timestamps remain null. Positions are observed
HTML attributes, not verified against the current diff. Coverage always states
that stored history, raw Markdown and stored PR association are incomplete or
unverified. A lookup with `status: "not_observed", comment: null` means the ID
was absent from this observation; it does not establish deletion or global absence.
PR metadata is read before and after the page; an observed change returns
`read_conflict` without partial data. These reads are not an atomic snapshot.

`add_pull_request_review_comment` requires `owner`, `repo`, `pull_number`, `body`,
`path`, the full lowercase current-head SHA-1 as `commit`, exactly one positive
`old_line` / `new_line`, and explicit `head_guard: "preflight"`. It supports
changed lines in added/modified ordinary text files on an open PR. It verifies
the pinned base/head patch before posting once, then reads the saved ID's own
raw Markdown, commit page and PR page. Success returns `comment`, `coverage`
and scoped `verification`; stored PR association and complete history remain
unverified. Subsequent get/full-view list calls return HTML only; summary lists
omit bodies explicitly.

The Web form trims body and path, so leading/trailing whitespace and unsupported
control characters are rejected before HTTP. Internal LF, indentation, tabs and
Markdown hard breaks are preserved. Context lines, whole-file deletion,
explicit rename/copy diffs, binary files and historical commits are unsupported.
The head check has a race window; `head_guard: "atomic"` is rejected before
HTTP. Read-only mode hides and rejects the tool. After an uncertain result,
inspect the structured operation target and any saved ID before retrying;
there is no automatic retry, ordinary-comment fallback or rollback.

See [the posting contract](docs/review-comment-posts.md),
[the read contract](docs/review-comment-reads.md) and
[P08 design](docs/inline-comment-design.md) for exact limits and the extension
required for complete history. Reproduce live stdio MCP checks with the
[contract probe](TESTING.md#review-comment-posts-p10).

### Structured Output Shape

MCP structured results are always JSON objects. List tools return their arrays under a stable field name:

| Tool | Result field |
|------|--------------|
| `list_repositories` | `repositories` |
| `list_branches` | `branches` |
| `list_issues` | `issues` |
| `list_issue_comments` | `comments` |
| `list_labels` | `labels` |
| `list_issue_labels` | `labels` |
| `list_milestones` | `milestones` |
| `list_pull_requests` | `pull_requests` |
| `list_pull_request_review_comments` | `comments` (also returns `coverage`) |
| `list_pull_request_commits` | `commits` (also `comparison`) |
| `list_pull_request_files` | `files` (also `comparison`) |

All eleven list tools accept optional `limit` (default 50, maximum 100), `cursor`
and `view` (`summary` by default, or `full`). Every page adds `has_more`,
`next_cursor` and `range`. Follow cursors with identical target, filters, limit
and view until `has_more` is false. Cursors expire after five minutes and can be
evicted earlier; invalid cursors fail before HTTP without a fresh search.

Summary lists omit Issue/PR bodies and repeated PR repository details; other
long display text, including ordinary comment previews, is bounded.
`_list.omitted_fields` / `_list.truncated_fields` identify removed/shortened
fields. Use detail getters or `view: "full"` for full models. Full view is also
paged and can reject an item that cannot fit; it adds no unknown review raw
text/history. Getters and writes retain their complete models.

Each list response is bounded to 64 KiB, including MCP's structured result and
text fallback, so a page can contain fewer than `limit` items. Range reports
the observed total, offsets, limit reason and source completeness. Continuations
serve a cached observation without HTTP; initial collection and later detail
reads are not an atomic snapshot. The last review page retains partial coverage.
See [the paging contract](docs/list-continuation.md) and
[migration notes](RELEASE.md#unreleased-p11-bounded-list-responses).

All tool inputs reject unknown fields before HTTP with an MCP invalid-parameters
error. In particular, `path`, `line` and `commit_id` do not turn an ordinary PR
comment into an inline comment. Send only fields advertised by the input schema.

Issue/PR bodies, comments, milestone/repository descriptions and merge commit
messages preserve the supplied text, including indentation, trailing spaces and
newlines. Required comments still reject whitespace-only text. Review posting
has the additional Web preservation constraints above. Identifiers,
titles, state and dates retain their documented normalization. Empty update
bodies/descriptions explicitly clear content; whitespace-only optional content
is preserved as content, not converted to a clear.

The standard GitBucket Issue web form trims body text. If a changed body has
leading/trailing whitespace, `update_issue` returns `unsupported_error` after its
snapshot read and before any writes, including accompanying title/state changes.
An unchanged body needs no content form, and REST-capable profiles preserve the
text. Milestone web content has the corresponding existing whitespace constraint.

Detail and full-view PR results retain `draft`, labels/assignees, `merged_by`, branch users and the
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
not every concurrent change can be detected. MCP response pages are separate
from those traversals; subsequent cursors serve the cached observation.

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
- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- `cargo test --locked`
- Rust 1.88.0 locked build/check, shell syntax and all Python tests

PRs also run affected Docker contract domains on 4.48.0 after fast checks;
docs-only changes skip live checks. `Required checks` aggregates required results.
Nightly/manual `E2E` runs all domains on 4.44.0 and 4.48.0. The shared runner
checks existing E2Es, actual stdio review/post/merge probes and PAT-only startup,
then removes the disposable stack. Only a safe summary artifact is uploaded.
See [the acceptance policy](docs/acceptance.md) for the scope and reproduction.

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

The same bootstrap flow is automated by the shared contract runner. `CI` provides
fast checks plus selected PR domains; `E2E` provides the full supported-version
matrix. Release packaging uses the same full acceptance matrix.

The bootstrap script starts a disposable GitBucket bound to `127.0.0.1`, creates a validation user, issues a personal access token, and provisions the repository. It writes shell-quoted `runtime.env` and a separate `runtime.env.json`, both mode `0600`, in the mode `0700` runtime directory. Both contain test credentials. Source only the generated shell file; CI imports JSON with `python3 scripts/e2e/runtime.py github "$GITHUB_ENV"`, not by stripping `export` from the shell file. Bootstrap removes its temporary cookie/response files on failure. Use `down.sh` to remove the stack and default runtime directory after testing.

A saved E2E token is reused only when `/api/v3/user` identifies the configured
validation user. A token for another account or an unreadable identity response
causes bootstrap to obtain a token for the configured user.

## Acknowledgements

This project exists thanks to [GitBucket](https://gitbucket.github.io/) and the community around it. Thank you for building and maintaining the software that made this MCP server worth creating.

## License

MIT
