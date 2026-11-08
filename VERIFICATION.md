# Verification Checklist

Use this checklist after installing `gitbucket-mcp-server` from GitHub Releases, `cargo install`, or a local build.

## 1. Confirm the binary location

Check that the command resolves to the binary you intended to install:

```bash
which gitbucket-mcp-server
gitbucket-mcp-server --help
gitbucket-mcp-server --version
```

On Windows, use:

```powershell
Get-Command gitbucket-mcp-server
gitbucket-mcp-server --help
gitbucket-mcp-server --version
```

Help/version exit successfully without reading configuration or contacting
GitBucket. Use one command at a time; unknown/combined commands exit with code 2.

## 2. Validate local configuration

Verify one of these is in place:

- `GITBUCKET_URL` and `GITBUCKET_TOKEN` are set
- A TOML file supplies each value absent from the environment. The path is
  `$GITBUCKET_MCP_CONFIG_DIR/config.toml` when overridden, otherwise the platform
  config directory's `gitbucket-mcp-server/config.toml` (`~/.config` by default
  on Linux, `~/Library/Application Support` on macOS, `%APPDATA%` on Windows).

Full URL/token env configuration ignores the file, including malformed unused
TOML. Partial env configuration requires a readable, valid file for missing
values. Explicit blank/invalid env values fail without fallback.

Optional web fallback credentials must come from environment variables only:

- `GITBUCKET_USERNAME`
- `GITBUCKET_PASSWORD`

Both must be set together and nonblank. Credentials alone do not enable Web
routes; select `GITBUCKET_API_PROFILE=standard` when those operations are needed.
PAT-only scope and exact routes are in the [README](README.md#gitbucket-api-profiles).

```bash
gitbucket-mcp-server --check-config
```

Expect exit code 0 and one JSON object with `status: "ok"`,
`mode: "configuration"`, `connection_checked: false`,
`web_session_checked: false`, the profile/access mode and whether Web credentials
are configured. This validates all startup settings and client construction
without any HTTP. Failure exits 1 with a safe error on stderr. No setting is
written or repaired by this check.

## 3. Explicitly check the PAT connection

```bash
gitbucket-mcp-server --check-connection
```

Expect exit 0 with `mode: "connection"`, `connection_checked: true` and
`web_session_checked: false`. The command makes only `GET /api/v3/user`, requires
a nonblank login and equality with any configured Web username, and does not
submit the password. It does not test repository permissions, writes, a Web
session, SSO or MFA. Failure exits 1. Diagnostics omit user identity, private
URLs and credentials even with dependency tracing requested in `RUST_LOG`.

For private HTTPS CAs, configure the server process's native trust store or
`SSL_CERT_FILE`/`SSL_CERT_DIR` as described in the
[CA instructions](README.md#https-and-custom-certificate-authorities). Those env
overrides replace native root discovery; include all required CAs. Avoid placing
tokens, passwords or instance URLs in shared diagnostic logs.

## 4. Check standalone waiting and MCP client wiring

Start the server directly:

```bash
gitbucket-mcp-server
```

Expected result:

- `stderr` prints `gitbucket-mcp-server starting over stdio; waiting for MCP initialization`
- stdout remains empty while the process waits for an MCP client on stdin
- readiness is not reported while waiting; this is expected

Stop it with `Ctrl+C`.

After adding the server to Claude Desktop, VS Code, or another MCP client,
configure the client to launch it without arguments:

- the client starts the process without a configuration error
- the client sends `initialize` and then `notifications/initialized`; only then
  does stderr report `gitbucket-mcp-server ready (MCP initialized)`
- the tool list is visible (32 normally, 16 with `GITBUCKET_READ_ONLY=true`)
- `get_authenticated_user` succeeds
- `list_repositories` or `get_repository` succeeds against your target GitBucket

Readiness proves the MCP handshake, while a successful authenticated read proves
the PAT connection. Standard Web operations verify PAT and cookie identities
when invoked; startup does not silently sign in.

## 5. Optional deeper verification

For repository developers or release verification:

```bash
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo +1.88.0 build --locked
cargo +1.88.0 check --locked --all-targets --all-features
```

For Docker-backed live E2E:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
cargo test --locked --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

Use a disposable local instance for live tests; they create and mutate data.
See [TESTING.md](TESTING.md#configuration-cli-and-minimum-rust-p13) for automated
real-process CLI, stdio and local-CA regressions and the minimum Rust setup.
