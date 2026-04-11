# Testing Guide

This repository uses layered tests so failures can be isolated quickly:

- Unit tests in `src/**` cover models, config loading, validation, and tool behavior.
- `tests/api_client_test.rs` verifies HTTP request and response handling with `wiremock`.
- `tests/mcp_server_test.rs` verifies MCP tool registration and tool calls over an in-memory transport, including structured MCP success/error payloads.
- `tests/e2e_test.rs` runs ignored smoke tests against a real GitBucket instance, including repository create-path coverage, label create/read/update/delete lifecycle coverage, milestone lifecycle coverage, Issue write paths, issue web fallback coverage, and pull request create/comment/merge coverage.

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
- `GITBUCKET_E2E_OWNER` for milestone, issue, and pull request tests against an existing repository
- `GITBUCKET_E2E_REPO` for milestone, issue, and pull request tests against an existing repository
- `GITBUCKET_E2E_GIT_USERNAME` / `GITBUCKET_E2E_GIT_PASSWORD` for pull request write-path tests that create and push temporary branches
- `GITBUCKET_E2E_WEB_USERNAME` / `GITBUCKET_E2E_WEB_PASSWORD` for explicit issue web-fallback credentials; when omitted, the E2E suite reuses git credentials
- `GITBUCKET_E2E_INSECURE_TLS=true` only when testing against local/self-signed HTTPS

Run the ignored suite explicitly:

```bash
cargo test --test e2e_test -- --ignored --nocapture
```

The write-path tests intentionally keep created repositories, Issues, comments, pull requests, and merged branches. Each run uses unique repo names, branch names, titles, file names, and comment bodies so reruns do not depend on cleanup.
Label E2E creates, reads, updates, and deletes a unique label within the test itself so it does not leave label fixtures behind.
Milestone E2E creates, updates, and deletes a unique milestone within the test itself so it does not leave milestone fixtures behind.
Current GitBucket Docker coverage verifies that `update_issue(state/title/body)` falls back through the web UI on the official `4.44.0` image when web credentials are available.
The API client also auto-paginates list endpoints with `per_page=100` until the last short page, so multi-page fixtures should be used when adding regression tests for list behavior.

## Docker E2E Flow

For a disposable local GitBucket:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

The bootstrap script starts GitBucket with Docker, creates a validation user, creates a personal access token, provisions an initialized target repository, and writes `./.tmp/e2e/runtime.env`, including the authenticated context for repository and milestone E2E, git-over-HTTP credentials for PR E2E, and web-fallback credentials for `update_issue`.

## GitHub Actions E2E

GitHub Actions keeps Docker-backed E2E separate from the fast `CI` workflow:

- `.github/workflows/ci.yml` runs `fmt`, `clippy`, and `cargo test` on every push and pull request.
- `.github/workflows/e2e.yml` runs on `workflow_dispatch` and a nightly schedule.

The `E2E` workflow validates the shell scripts, runs `./scripts/e2e/bootstrap.sh`, loads `./.tmp/e2e/runtime.env`, executes `cargo test --test e2e_test -- --ignored --nocapture`, and always tears the Docker stack down.

## Adding Tests

- Put API contract coverage in `tests/api_client_test.rs`.
- Put MCP protocol and tool-surface coverage in `tests/mcp_server_test.rs`.
- Keep tool-module unit tests near the implementation in `src/tools/*.rs`.
- Prefer write-path E2E only when setup is explicit and reruns are collision-safe; cleanup may be best-effort if tests use dedicated disposable data such as uniquely named repositories and branches.
- Prefer asserting structured MCP payloads (`structured_content`, `is_error`) instead of `"Error: ..."` text.
- Never hardcode real tokens or instance URLs in tests or fixtures.
