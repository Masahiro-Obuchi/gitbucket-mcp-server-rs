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

The config file is created with `0600` permissions (owner-only read/write) to protect the token. Web fallback credentials are intentionally **not** read from `config.toml`; set `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` via environment variables only.

The config directory can be overridden with the `GITBUCKET_MCP_CONFIG_DIR` environment variable.

### Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `GITBUCKET_URL` | ✅* | GitBucket instance URL | `https://gitbucket.example.com` |
| `GITBUCKET_TOKEN` | ✅* | Personal Access Token | `abc123...` |
| `GITBUCKET_USERNAME` | ❌ | GitBucket username for web fallback operations | `alice` |
| `GITBUCKET_PASSWORD` | ❌ | GitBucket password for web fallback operations | `secret-pass` |
| `GITBUCKET_MCP_CONFIG_DIR` | ❌ | Override config directory | `/custom/path` |

\* Required if not set in config file. Environment variables override config file values. `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are optional, but must be set together when used.

`GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are used only when this MCP server
falls back to GitBucket's web UI for operations that are unavailable through the
REST API. They are **not** used for Git over HTTP operations such as `git clone`,
`git fetch`, or `git push`. Configure your Git credential helper separately if
Git commands prompt for a username or password.

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
export GITBUCKET_USERNAME="alice"         # optional, for web fallback operations
export GITBUCKET_PASSWORD="secret-pass"   # optional, env-only, not used by Git-over-HTTP commands
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
        "GITBUCKET_USERNAME": "${input:gitbucket_username}",
        "GITBUCKET_PASSWORD": "${input:gitbucket_password}"
      }
    }
  }
}
```

Only add `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` when you want web
fallback enabled. They must be set together; leaving one blank or setting only
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
| `update_label` | Update label name, color, or description; REST-incompatible GitBucket instances fall back for name/color |
| `delete_label` | Delete a label |

### Milestones

| Tool | Description |
|------|-------------|
| `list_milestones` | List milestones for a repository |
| `get_milestone` | Get milestone details |
| `create_milestone` | Create a new milestone |
| `update_milestone` | Update milestone fields and state |
| `delete_milestone` | Delete a milestone |

### Pull Requests

| Tool | Description |
|------|-------------|
| `list_pull_requests` | List pull requests (filterable by state) |
| `get_pull_request` | Get PR details |
| `create_pull_request` | Create a new pull request |
| `update_pull_request` | Update PR state, title, body, or base branch |
| `merge_pull_request` | Merge a pull request |
| `add_pull_request_comment` | Add a comment to a pull request |

### User

| Tool | Description |
|------|-------------|
| `get_authenticated_user` | Get the authenticated user's info |
| `get_user` | Get a user by username |

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

For GitBucket versions that do not expose the REST label update endpoint, `update_label` uses the web fallback for name/color changes. Standard GitBucket does not support label description updates through that fallback; description-only updates return an unsupported error, while name/color updates can proceed even if a description is included in the request.

GitBucket's issue API does not expose `closed_at` on some versions. When a closed issue has no `closed_at` but does have `updated_at`, this server fills `closed_at` with `updated_at` as a best-effort GitHub-compatible value.

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
cargo test --test e2e_test -- --ignored --nocapture
```

Optional variables:

- `GITBUCKET_E2E_OWNER`: defaults to the authenticated user for `list_repositories`
- `GITBUCKET_E2E_REPO`: required for milestone, issue, and pull request E2E against an existing repository
- `GITBUCKET_E2E_GIT_USERNAME` / `GITBUCKET_E2E_GIT_PASSWORD`: required for pull request write-path E2E because the tests create and push temporary branches over HTTP
- `GITBUCKET_E2E_WEB_USERNAME` / `GITBUCKET_E2E_WEB_PASSWORD`: optional explicit credentials for web fallback tests; if omitted, E2E reuses the git credentials
- `GITBUCKET_E2E_INSECURE_TLS=true`: allow self-signed or locally trusted HTTPS certificates during E2E runs
- Write-path E2E tests leave created repositories, Issues, comments, pull requests, and merged branches in place; they use unique repo names, branch names, titles, and bodies to avoid collisions across reruns
- Milestone E2E creates, updates, and deletes a unique milestone within the same test so reruns do not accumulate milestone fixtures

### Docker E2E Bootstrap

You can provision a disposable local GitBucket instance for the E2E suite:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

The same bootstrap flow is also automated in GitHub Actions through `.github/workflows/e2e.yml`. Use the regular `CI` workflow for fast feedback and the `E2E` workflow for full Docker-backed smoke coverage.

The bootstrap script starts GitBucket with Docker, creates a validation user, issues a personal access token, creates an initialized target repository, and writes `./.tmp/e2e/runtime.env` with the `GITBUCKET_E2E_*` variables expected by `tests/e2e_test.rs`, including git-over-HTTP credentials for pull request E2E and the authenticated context needed for repository create-path E2E.

## Acknowledgements

This project exists thanks to [GitBucket](https://gitbucket.github.io/) and the community around it. Thank you for building and maintaining the software that made this MCP server worth creating.

## License

MIT
