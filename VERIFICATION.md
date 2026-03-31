# Verification Checklist

Use this checklist after installing `gitbucket-mcp-server` from GitHub Releases, `cargo install`, or a local build.

## 1. Confirm the binary location

Check that the command resolves to the binary you intended to install:

```bash
which gitbucket-mcp-server
gitbucket-mcp-server --help
```

On Windows, use:

```powershell
Get-Command gitbucket-mcp-server
gitbucket-mcp-server --help
```

## 2. Confirm configuration is available

Verify one of these is in place:

- `GITBUCKET_URL` and `GITBUCKET_TOKEN` are set
- `~/.config/gitbucket-mcp-server/config.toml` exists and contains `url` and `token`

Optional web fallback credentials must come from environment variables only:

- `GITBUCKET_USERNAME`
- `GITBUCKET_PASSWORD`

## 3. Run a startup smoke check

Start the server directly:

```bash
gitbucket-mcp-server
```

Expected result:

- `stderr` prints a startup line such as `gitbucket-mcp-server starting for ...`
- `stderr` then prints `gitbucket-mcp-server ready`
- the process stays running, waiting for MCP input on `stdin`

Stop it with `Ctrl+C`.

## 4. Verify MCP client wiring

After adding the server to Claude Desktop, VS Code, or another MCP client:

- the client starts the process without a configuration error
- the tool list is visible
- `get_authenticated_user` succeeds
- `list_repositories` or `get_repository` succeeds against your target GitBucket

## 5. Optional deeper verification

For repository developers or release verification:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

For Docker-backed live E2E:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```
