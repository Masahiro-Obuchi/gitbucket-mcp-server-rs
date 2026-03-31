# GitBucket MCP Server
[![CI](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/ci.yml)
[![E2E](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/e2e.yml/badge.svg)](https://github.com/Masahiro-Obuchi/gitbucket-mcp-server-rs/actions/workflows/e2e.yml)

A [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server for [GitBucket](https://gitbucket.github.io/), written in Rust.

This server enables AI assistants (Claude Desktop, GitHub Copilot, etc.) to interact with GitBucket repositories, issues, and pull requests through the MCP protocol.
This is an unofficial community project and is not affiliated with the GitBucket project.

## Features

- **Repository Management**: List, view, create, fork repositories and list branches
- **Issue Tracking**: List, view, create, update issues; manage comments
- **Pull Requests**: List, view, create, merge PRs; add comments
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

### cargo install

This project is not published to crates.io yet, so install from Git:

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
| `GITBUCKET_USERNAME` | ❌ | GitBucket username for issue state web fallback | `alice` |
| `GITBUCKET_PASSWORD` | ❌ | GitBucket password for issue state web fallback | `secret-pass` |
| `GITBUCKET_MCP_CONFIG_DIR` | ❌ | Override config directory | `/custom/path` |

\* Required if not set in config file. Environment variables override config file values. `GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD` are optional, but must be set together when used.

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
export GITBUCKET_USERNAME="alice"         # optional, for issue state web fallback only
export GITBUCKET_PASSWORD="secret-pass"   # optional, env-only
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

Add to your VS Code settings (`.vscode/mcp.json`):

```json
{
  "servers": {
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

### Pull Requests

| Tool | Description |
|------|-------------|
| `list_pull_requests` | List pull requests (filterable by state) |
| `get_pull_request` | Get PR details |
| `create_pull_request` | Create a new pull request |
| `merge_pull_request` | Merge a pull request |
| `add_pull_request_comment` | Add a comment to a pull request |

### User

| Tool | Description |
|------|-------------|
| `get_authenticated_user` | Get the authenticated user's info |
| `get_user` | Get a user by username |

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

The separate `E2E` workflow is reserved for `workflow_dispatch` and nightly runs. It boots a disposable GitBucket with Docker, exports `GITBUCKET_E2E_*`, runs `cargo test --test e2e_test -- --ignored --nocapture`, and always tears the stack down afterward.

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
│   ├── issue.rs
│   ├── pull_request.rs
│   └── user.rs
├── models/          # API request/response types
│   ├── user.rs
│   ├── repository.rs
│   ├── issue.rs
│   ├── pull_request.rs
│   └── comment.rs
└── tools/           # MCP tool definitions
    ├── repository.rs
    ├── issue.rs
    ├── pull_request.rs
    └── user.rs
```

## Testing Notes

- `tests/api_client_test.rs` uses `wiremock` to validate GitBucket API requests and responses.
- `tests/mcp_server_test.rs` exercises the MCP tool surface over an in-memory transport.
- `tests/e2e_test.rs` provides ignored smoke tests against a real GitBucket instance, including repository creation with branch discovery, Issue write paths, state-only web fallback coverage, and pull request create/comment/merge flows.
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
- `GITBUCKET_E2E_REPO`: required for issue and pull request E2E against an existing repository
- `GITBUCKET_E2E_GIT_USERNAME` / `GITBUCKET_E2E_GIT_PASSWORD`: required for pull request write-path E2E because the tests create and push temporary branches over HTTP
- `GITBUCKET_E2E_WEB_USERNAME` / `GITBUCKET_E2E_WEB_PASSWORD`: optional explicit credentials for `update_issue` web fallback; if omitted, E2E reuses the git credentials
- `GITBUCKET_E2E_INSECURE_TLS=true`: allow self-signed or locally trusted HTTPS certificates during E2E runs
- Write-path E2E tests leave created repositories, Issues, comments, pull requests, and merged branches in place; they use unique repo names, branch names, titles, and bodies to avoid collisions across reruns

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
