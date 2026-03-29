# Testing Guide

This repository uses layered tests so failures can be isolated quickly:

- Unit tests in `src/**` cover models, config loading, validation, and tool behavior.
- `tests/api_client_test.rs` verifies HTTP request and response handling with `wiremock`.
- `tests/mcp_server_test.rs` verifies MCP tool registration and tool calls over an in-memory transport.
- `tests/e2e_test.rs` runs ignored read-only smoke tests against a real GitBucket instance.

## Common Commands

Use these commands during normal development:

```bash
# All non-ignored tests
cargo test

# Fast checks that do not require wiremock port binding or a live GitBucket
cargo test --lib
cargo test --test mcp_server_test

# Compile the ignored E2E target without running it
cargo test --test e2e_test

# Lint with the same strictness as CI
cargo clippy --all-targets --all-features -- -D warnings
```

## Live E2E Tests

`tests/e2e_test.rs` expects these environment variables:

- `GITBUCKET_E2E_URL`
- `GITBUCKET_E2E_TOKEN`
- `GITBUCKET_E2E_OWNER` for repository-scoped tests
- `GITBUCKET_E2E_REPO` for repository-scoped tests
- `GITBUCKET_E2E_INSECURE_TLS=true` only when testing against local/self-signed HTTPS

Run the ignored suite explicitly:

```bash
cargo test --test e2e_test -- --ignored --nocapture
```

## Docker E2E Flow

For a disposable local GitBucket:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

The bootstrap script starts GitBucket with Docker, creates a validation user, creates a personal access token, provisions the target repository, and writes `./.tmp/e2e/runtime.env`.

## GitHub Actions E2E

GitHub Actions keeps Docker-backed E2E separate from the fast `CI` workflow:

- `.github/workflows/ci.yml` runs `fmt`, `clippy`, and `cargo test` on every push and pull request.
- `.github/workflows/e2e.yml` runs on `workflow_dispatch` and a nightly schedule.

The `E2E` workflow validates the shell scripts, runs `./scripts/e2e/bootstrap.sh`, loads `./.tmp/e2e/runtime.env`, executes `cargo test --test e2e_test -- --ignored --nocapture`, and always tears the Docker stack down.

## Adding Tests

- Put API contract coverage in `tests/api_client_test.rs`.
- Put MCP protocol and tool-surface coverage in `tests/mcp_server_test.rs`.
- Keep tool-module unit tests near the implementation in `src/tools/*.rs`.
- Prefer read-only E2E coverage first; add write-path E2E only when setup and cleanup are explicit and repeatable.
- Never hardcode real tokens or instance URLs in tests or fixtures.
