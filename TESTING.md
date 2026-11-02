# Testing Guide

This repository uses layered tests so failures can be isolated quickly:

- Unit tests in `src/**` cover models, config loading, validation, and tool behavior.
- `tests/api_client_test.rs` verifies HTTP request and response handling with `wiremock`.
- `tests/list_api_test.rs` covers endpoint-specific list contracts, boundary
  counts, duplicate identities, and failure/limit behavior.
- `tests/milestone_api_test.rs` models GitBucket's replacement-style PATCH and
  verifies field preservation, date normalization, explicit clears, web date
  constraints, ignored responses, and operation-stage failures.
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

## Operation Routing Regression Tests

```bash
cargo test --test routing_test
```

Default clients (with web credentials present) retain unknown 404 errors without signing in or sending secondary writes. Profile tests distinguish REST rename/PR updates from known web Issue/same-name color/restricted-title operations. They cover 401/403/409/422 rejection, 408/5xx unknown outcomes, removed profile rejection, strict label-write schema/argument rejection before all HTTP, no-op color normalization, incorrect snapshots, and ignored REST/web responses.

Whitespace-padded label regressions model the web form trimming a name during a color change. Standard web updates reject these names before sign-in or mutation; REST create/rename operations and unchanged labels still preserve them.

Regression tests for deleted legacy write paths have been removed. The remaining standard web tests select the supported contract directly, without rejected REST-write fixtures. Label fixtures no longer claim standard description persistence. The label lifecycle E2E verifies a same-name color update and confirms that a rejected combined description/name/color request leaves the stored label unchanged.

## HTTP Budget Regression Tests

```bash
cargo test --test http_budget_test
```

Local TCP/HTTP/TLS mocks cover delayed headers, delayed body chunks, TLS handshake stalls, declared/chunked/unframed oversized bodies, exact-size acceptance, and explicit longer merge budgets. Mutating requests that arrive before a disconnect, malformed JSON, timeout or size failure return an unknown outcome without replay. Web sign-in, label HTML and web mutations use the same limits.

REST redirect regressions cover POST/PATCH/PUT/DELETE across HTTP 301/302/303/307/308. Each write stops at the original 3xx with an unknown outcome, without reaching the redirect target or triggering issue compatibility fallback. A separate read regression preserves safe GET redirects.

MCP protocol tests exercise a whole-call deadline across multiple pages, concurrent read/write isolation, clearing a definitively rejected attempt, cancellation notifications and verification reads after web writes. The server classification is also tested before rmcp's client discards a cancelled response. A delayed custom API verifies conservative write outcomes without transport tracking. Unit tests validate settings, env overrides and secret-free configuration diagnostics.

The default budgets were exercised against official Docker GitBucket 4.44.0 with all 22 existing E2Es, including the two large-list fixtures. This does not establish hard CPU deadlines or compatibility with every production workload; adjust the documented env settings where needed.

## Safety Regression Tests

```bash
cargo test --test safety_test
python3 scripts/e2e/test_runtime.py
bash -n scripts/e2e/common.sh scripts/e2e/bootstrap.sh scripts/e2e/down.sh
```

HTTP mocks cover invalid identifiers without requests, exact label targets, IPv6/context URLs, redirect boundaries, and secret-free API/web/tool diagnostics. Process tests check startup stderr with malformed TOML and credential-bearing URLs. Unix config tests cover private creation, replacement of permissive files, inode replacement, symlinks, and preservation after failed saves.

Redirect regressions include contexts ending in `/api/v3`, blocking out-of-context
307/308 password replays while allowing sign-in and writes inside that context.

POST query tests verify the Issue `action=close` fixture call and separately
encoded query values; the large-list E2E uses the same `post_with_query` API.

JSON diagnostic tests also traverse `Error::source()` to prevent raw response
field values from leaking through error-chain logging.

The Python tests round-trip spaces, quotes, backslashes, dollars, backticks and newlines through shell and GitHub environment formats. They also verify `0700`/`0600`, symlink refusal, replacement failure cleanup, and bootstrap failure cleanup. Bootstrap serializes custom user/password values as JSON rather than interpolating them into JSON strings.

Token-reuse tests run bootstrap with stubbed HTTP/Docker commands and verify that
only the configured account's token is reused, while other-account, malformed,
or missing identities cause a new token to be obtained without logging secrets.

## Live E2E Tests

Milestone partial-update regressions have a PAT-only E2E and a separate
web-compatibility E2E for Japanese titles. Against a disposable instance, run:

```bash
cargo test --test e2e_test test_e2e_milestone_ -- --ignored --nocapture
```

The PAT-only test disables web credentials explicitly and verifies closed-state
and field preservation, independent changes, date offsets, leap days, clearing,
and deletion. The web test requires credentials and verifies Japanese titles,
state/content changes, date clearing, and rejection of non-midnight dates
without changing the stored value. Date round trips assume the disposable
server's UTC timezone; differing web-server timezone behavior is a documented
limitation. Both tests remove their milestones after successful verification.

PR update regressions are covered with a stateful mock of GitBucket's coupled
title/body update behavior. The tests cover independent content updates, empty
and absent bodies, state-only updates, invalid/ignored base changes, unverified
HTTP 200 responses, and unknown write outcomes without retries.
Standard web regressions distinguish failures before and after writing, and preserve unknown outcomes without replay. PRs use their dedicated REST endpoint; 404 errors do not activate Issue or Web writes. Transport tests cover rejected requests versus timeout/server failures.

Against a disposable instance with the E2E variables below configured, run:

```bash
cargo test --test e2e_test test_e2e_update_pull_request_preserves_unspecified_fields -- --ignored --nocapture
```

This test seeds a branch using Git credentials, then explicitly disables web
credentials for MCP calls. It verifies title-only/body-only updates, Markdown
whitespace, body clearing, close/reopen, base validation, and a final independent
read. It leaves its uniquely named branch and PR in the disposable repository.

`tests/e2e_test.rs` expects these environment variables:

- `GITBUCKET_E2E_URL`
- `GITBUCKET_E2E_TOKEN`
- `GITBUCKET_E2E_OWNER` for milestone, issue, and pull request tests against an existing repository
- `GITBUCKET_E2E_REPO` for milestone, issue, and pull request tests against an existing repository
- `GITBUCKET_E2E_GIT_USERNAME` / `GITBUCKET_E2E_GIT_PASSWORD` for pull request write-path tests that create and push temporary branches
- `GITBUCKET_E2E_WEB_USERNAME` / `GITBUCKET_E2E_WEB_PASSWORD` for explicit issue web-fallback credentials; when omitted, the E2E suite reuses git credentials
- `GITBUCKET_E2E_API_PROFILE=standard` with web credentials for the full suite against official GitBucket; defaults to `rest-only`
- `GITBUCKET_E2E_INSECURE_TLS=true` only when testing against local/self-signed HTTPS

Run the ignored suite explicitly:

```bash
export GITBUCKET_E2E_API_PROFILE=standard
cargo test --test e2e_test -- --ignored --nocapture
```

The write-path tests intentionally keep created repositories, Issues, comments, pull requests, and merged branches. Each run uses unique repo names, branch names, titles, file names, and comment bodies so reruns do not depend on cleanup.
Label E2E creates, reads, updates, and deletes a unique label within the test itself so it does not leave label fixtures behind.
Milestone E2E creates, updates, and deletes a unique milestone within the test itself so it does not leave milestone fixtures behind.
The standard profile selects Web directly for Issue updates and same-name label color changes on the tested official `4.44.0` and `4.48.0` images. Label renames use REST; failed REST writes never activate a secondary route. Web credentials alone do not select the standard profile.
Issue/PR fixtures use GitBucket's fixed 25-item pages, followed by an empty page.
Repository, branch, label, milestone, and issue comment fixtures return the full
response in one request and must not expect pagination parameters. The list
contract tests cover 0/1/24/25/26/50/51 paged items and 0/1/99/100/101 unpaged
items, all Issue/PR state filters, short non-final pages, changed/reordered
duplicate pages, overlapping pages, later failures, and the 100-request guard.

Two additional E2E tests exercise large lists through MCP, explicitly without
web credentials. Run them **only against a disposable GitBucket**:

```bash
cargo test --test e2e_test test_e2e_list_paged_resources_over_25_items -- --ignored --nocapture
cargo test --test e2e_test test_e2e_list_unpaged_resources_over_100_items -- --ignored --nocapture
```

The first creates a unique repository with 51 issues (26 closed, 25 open) and
26 PRs, then compares the complete identifier sets. The second creates another
repository with 101 each of labels, milestones, comments, and branches and
checks that every created identifier is returned without duplicates. It also
checks repository discovery; 101-repository responses and organization fallback
are covered by HTTP mocks. These tests leave their repositories and fixtures in
place. The paged test requires Git credentials to seed a commit; the unpaged test
uses REST to seed references and needs only a token with repository write access.

## Docker E2E Flow

For a disposable local GitBucket:

```bash
./scripts/e2e/bootstrap.sh
source ./.tmp/e2e/runtime.env
export GITBUCKET_E2E_API_PROFILE=standard
cargo test --test e2e_test -- --ignored --nocapture
./scripts/e2e/down.sh
```

The bootstrap script starts GitBucket with Docker, creates a validation user, creates a personal access token, provisions an initialized target repository, and writes `./.tmp/e2e/runtime.env`, including the authenticated context for repository and milestone E2E, git-over-HTTP credentials for PR E2E, and web-fallback credentials for `update_issue`.

Runtime directories are `0700`; both the shell-quoted `runtime.env` and canonical
`runtime.env.json` are `0600`. The JSON file is used to reuse a token and import CI
variables without executing a saved shell script. Do not feed shell quoting to
`GITHUB_ENV`. Runtime files contain disposable credentials and are removed with
the default runtime directory by `down.sh`. If `GITBUCKET_E2E_ENV_FILE` points
outside that directory, remove both custom files explicitly. Docker's port is
bound to `127.0.0.1` by default.

For a fresh disposable 4.48.0 stack, set `GITBUCKET_E2E_IMAGE=ghcr.io/gitbucket/gitbucket:4.48.0` for bootstrap and explicitly set `GITBUCKET_E2E_API_PROFILE=standard` for the tests. Stop/remove any previous stack first or use separate Compose project, port and runtime-directory overrides. Image choice does not infer the profile; both tested versions use the same `standard` contract. The E2E client defaults to `rest-only` when no profile is supplied; tests requiring web operations need an appropriate explicit profile. The shared standard profile is tested against both versions with the full 22-case suite.

## GitHub Actions E2E

GitHub Actions keeps Docker-backed E2E separate from the fast `CI` workflow:

- `.github/workflows/ci.yml` runs `fmt`, `clippy`, `cargo test`, shell syntax checks, and Python runtime-file tests on every push and pull request.
- `.github/workflows/e2e.yml` runs on `workflow_dispatch` and a nightly schedule.

The E2E and release workflows explicitly select the standard profile for their default Docker image. The `E2E` workflow validates the scripts, runs `./scripts/e2e/bootstrap.sh`, imports `runtime.env.json` with `python3 scripts/e2e/runtime.py github "$GITHUB_ENV"`, executes `cargo test --test e2e_test -- --ignored --nocapture`, and always tears the Docker stack down.

## Adding Tests

- Put API contract coverage in `tests/api_client_test.rs`.
- Put MCP protocol and tool-surface coverage in `tests/mcp_server_test.rs`.
- Keep tool-module unit tests near the implementation in `src/tools/*.rs`.
- Prefer write-path E2E only when setup is explicit and reruns are collision-safe; cleanup may be best-effort if tests use dedicated disposable data such as uniquely named repositories and branches.
- Prefer asserting structured MCP payloads (`structured_content`, `is_error`) instead of `"Error: ..."` text.
- Never hardcode real tokens or instance URLs in tests or fixtures.
