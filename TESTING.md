# Testing Guide

This repository uses layered tests so failures can be isolated quickly:

- Unit tests in `src/**` cover models, config loading, validation, and tool behavior.
- `tests/api_client_test.rs` verifies HTTP request and response handling with `wiremock`.
- `tests/list_api_test.rs` covers endpoint-specific list contracts, boundary
  counts, duplicate identities, and failure/limit behavior.
- `tests/milestone_api_test.rs` models GitBucket's replacement-style PATCH and
  verifies field preservation, date normalization, explicit clears, web date
  constraints, ignored responses, and operation-stage failures.
- `tests/operation_outcome_test.rs` verifies partial progress, target/route/cause information and cancellation/deadline preservation over actual HTTP mocks and MCP dispatch.
- `tests/web_session_test.rs` covers verified PAT/cookie identities, expired sessions, strict redirects, partial updates and deletion verification.
- `tests/mcp_server_test.rs` verifies MCP tool registration and tool calls over an in-memory transport, including structured MCP success/error payloads.
- `tests/tool_semantics_test.rs` checks all tool schemas and unknown-argument
  rejection before HTTP, inline-argument refusal, original Markdown/commit text,
  response metadata, unknown vs empty/false values, closure dates, annotations,
  and every write blocked through both MCP and direct Rust handlers in read-only mode.
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

## MCP input and model semantics

```bash
cargo test --test tool_semantics_test
```

Synthetic response fixtures in `tests/fixtures/gitbucket/` cite the exact official
4.44.0/4.48.0 model sources. Tests check that those fields survive both get and
list calls, while missing metadata stays null and explicit empty arrays/false
remain distinguishable. Closed Issue updates do not synthesize closure dates.

All 29 tool schemas reject additional properties. MCP tests send otherwise-valid
arguments plus an unknown field and require invalid-parameters errors without
HTTP. Inline-location arguments cannot silently create a normal PR comment.
Whitespace tests cover required blank comments, optional content, code blocks,
hard line breaks and trailing newlines. Standard web Issue updates reject a
lossy body change before other fields are written, while an unchanged raw body
skips the content form. Read-only tests hide all 15 writes, call each through MCP
and the public Rust handler, and verify that only the permitted read reached HTTP.

Against a disposable standard-profile GitBucket with Git and web credentials:

```bash
cargo test --test e2e_test test_e2e_input_content_metadata_and_read_only -- --ignored --nocapture
```

The test preserves Markdown through Issue creation, comments and PR updates;
checks a real draft PR, null mergeability, metadata URLs and missing closed_at;
rejects lossy standard web content and inline arguments; and verifies read-only
listing/reads and blocked comments without changing conversation contents. It
leaves its unique Issue/PR/branch fixtures in the disposable repository.

## Operation Routing Regression Tests

```bash
cargo test --test routing_test
```

Default clients (with web credentials present) retain unknown 404 errors without signing in or sending secondary writes. Profile tests distinguish REST rename/PR updates from known web Issue/same-name color/restricted-title operations. They cover 401/403/409/422 rejection, 408/5xx unknown outcomes, removed profile rejection, strict label-write schema/argument rejection before all HTTP, no-op color normalization, incorrect snapshots, and ignored REST/web responses.

Whitespace-padded label regressions model the web form trimming a name during a color change. Standard web updates reject these names before sign-in or mutation; REST create/rename operations and unchanged labels still preserve them.

Regression tests for deleted legacy write paths have been removed. The remaining standard web tests select the supported contract directly, without rejected REST-write fixtures. Label fixtures no longer claim standard description persistence. The label lifecycle E2E verifies a same-name color update and confirms that a rejected combined description/name/color request leaves the stored label unchanged.

## Operation Outcome Regression Tests

```bash
cargo test --test operation_outcome_test
```

Stateful HTTP fixtures verify Issue title success followed by body failure, PR partial HTTP 200 responses, wrong response identity, preserved-field intent, milestone content validation before state changes, and ambiguous creation candidate IDs. MCP dispatch tests interrupt a later Issue field with both a deadline and cancellation and verify that earlier field evidence survives while later writes stop. REST 400/401/403/404/409/422 rejections remain distinct from 408/5xx uncertainty, with safe nested causes and no replay. Error payloads must not contain form secrets, changed titles/bodies or raw server error text.

Preflight no-op regressions retain matching Issue and milestone fields as `unchanged` after a later write fails or its verification returns 403. MCP deadline/cancellation cases preserve unchanged Issue fields without reporting partial application. A failed milestone content write marks every submitted content field unknown again, including values that matched during preflight.

Existing transport/service regressions assert the new operation status explicitly, including unverified responses, expired authentication before the first write, verification read failures and deletion checks. Successful tool payloads are checked by the existing MCP suite. Live verification passed all 22 E2Es on each official GitBucket 4.44.0 and 4.48.0 image using the standard profile.

## Web Session Regression Tests

```bash
cargo test --test web_session_test
```

Mocks assert that a configured account mismatch prevents password submission, cookie-only identity checks receive no PAT header, and differing/expired sessions prevent resource writes. They cover anonymous/SSO HTTP 200 pages, Jetty URL session stripping, 307/308 without credential/form replay, rejection of unrelated or mutating redirect destinations, ignored Issue writes stopping later fields, and account changes after a first applied field. Authentication expiry before the first write remains `authentication_error` across Issue and milestone services, with a structured MCP error; a prior verified field preserves `operation_partial` when a later field is not attempted.

HTML parser unit tests cover Unicode, encoded query values, HTML entities, full-name matching, numeric row boundaries, duplicate/foreign label rows, sign-in forms and nonempty validation markers. Deletion regressions check a successful DELETE followed by present, absent, forbidden or failed GET results without replay. Existing service tests cover HTTP 200 validation pages and ambiguity with existing same-title milestones.

Form-action regressions distinguish real authentication endpoints from ordinary forms in repositories such as `oauth-tools` and contexts such as `/oidc-context`, including absolute/relative actions and contexts ending in `/api/v3`. They cover label lookup, label write responses and milestone redirects, while actual sign-in forms still fail for resource reads and writes. Invalid label lookup targets return local input errors without checking even an expired session over HTTP.

The verified web-session implementation passed all 22 E2Es on each official Docker image, GitBucket 4.44.0 and 4.48.0, including Issue title/body/state, same-name label color, restricted milestone titles and verified REST deletion. These runs use disposable password-based accounts. SSO/MFA-only deployments require a supported password session for web routes; they are not covered by those runs.

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

Redirect regressions include contexts ending in `/api/v3`, preserving valid sign-in and read redirects inside that context while blocking escapes. Web 307/308 are rejected without any password/form replay, including same-origin destinations.

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
