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
- `tests/cli_test.rs` starts the built binary to verify configuration-free CLI,
  local/explicit-HTTP diagnostics, real stdio initialization and shared CA trust.
- `tests/tool_semantics_test.rs` checks all tool schemas and unknown-argument
  rejection before HTTP, inline-argument refusal, original Markdown/commit text,
  response metadata, unknown vs empty/false values, closure dates, annotations,
  and every write blocked through both MCP and direct Rust handlers in read-only mode.
- `tests/e2e_test.rs` runs ignored smoke tests against a real GitBucket instance, including repository create-path coverage, label create/read/update/delete lifecycle coverage, milestone lifecycle coverage, Issue write paths, issue web fallback coverage, and pull request create/comment/merge coverage.

## PR change reads (P15)

Run cargo test --locked --test pull_request_changes_test plus the parser unit
suite. The tests cover unpaged commit reads, full messages/unknown linked users,
file kinds, exact Unicode/raw patch chunks, source and MCP response limits,
wrong identities, read conflicts and cursor/filter binding with no continuation
HTTP. Global schema/annotation/read-only tests include all three new tools.

The core acceptance domain runs scripts/e2e/probe_changes.py through actual
stdio against a fresh private repository. It compares 28 commits/full messages
with Git, advances head during a cached read, compares text/mode/binary/deletion/
rename/tab/Unicode file metadata with Git, reconstructs the native patch and
reads after merge. See [the contract](docs/pull-request-changes.md).

## Common Commands

Use these commands during normal development:

```bash
# All non-ignored tests
cargo test --locked

# Fast checks that do not require wiremock port binding or a live GitBucket
cargo test --lib
cargo test --test mcp_server_test

# Compile the ignored E2E target without running it
cargo test --test e2e_test

# Lint with the same strictness as CI
cargo clippy --locked --all-targets --all-features -- -D warnings
```

## Configuration, CLI and minimum Rust (P13)

```bash
cargo test --locked --lib config::tests
cargo test --locked --test cli_test
rustup toolchain install 1.88.0 --profile minimal
cargo +1.88.0 build --locked
cargo +1.88.0 check --locked --all-targets --all-features
```

Configuration tests isolate and serialize environment changes. They cover full,
partial and absent URL/token env values against valid/missing/malformed TOML,
explicit blank/invalid UTF-8 rejection, paired Web credentials and unused paths.
Existing Unix file tests continue to check private atomic replacement and
symlink/non-regular rejection.

The ten subprocess tests check help/version before invalid config/runtime/logging,
argument exit codes, local diagnostics without any HTTP, explicit PAT identity
GETs without password sign-in, status/identity failures without secret output,
and actual stdin/stdout JSON-RPC. Readiness must wait for both initialization
messages; malformed initialization cannot echo the client's private payload.
Tests need loopback port binding but no external GitBucket or real credentials.

The local TLS server uses the synthetic localhost identity in
[`tests/fixtures/tls`](tests/fixtures/tls/README.md). An untrusted connection fails;
`SSL_CERT_FILE` plus an isolated `SSL_CERT_DIR` then trusts the test CA. The same
process proves diagnostics, a REST write and a verified Web cookie session use
that CA policy. No TLS verification bypass is used. Certificates and the server
key are public test fixtures; the CA signing key is discarded.

The locked graph includes dependencies requiring Rust 1.88 (including
`cookie_store`, `darling` and `time`). The minimum is checked by an actual locked
build and all-targets/all-features check on Linux, rather than inferred from the
crate's edition. Stable CI runs the complete normal suite. Keep minimum checks
when changing the lockfile; they do not claim cross-platform MSRV execution.

## MCP input and model semantics

```bash
cargo test --test tool_semantics_test
```

Synthetic response fixtures in `tests/fixtures/gitbucket/` cite the exact official
4.44.0/4.48.0 model sources. Tests check that those fields survive both get and
list calls, while missing metadata stays null and explicit empty arrays/false
remain distinguishable. Closed Issue updates do not synthesize closure dates.

All 42 tool schemas reject additional properties. MCP tests send otherwise-valid
arguments plus an unknown field and require invalid-parameters errors without
HTTP. Inline-location arguments cannot silently create a normal PR comment.
Whitespace tests cover required blank comments, optional content, code blocks,
hard line breaks and trailing newlines. Standard web Issue updates reject a
lossy body change before other fields are written, while an unchanged raw body
skips the content form. Read-only tests hide all 21 writes, call each through MCP
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

## Pull request merges (P12)

```bash
cargo test --test merge_test
```

Eight HTTP/MCP tests cover merge/squash/rebase wire names, ordinary merge versus
explicit expected-head preflight, zero-HTTP atomic/invalid inputs, draft/unknown
draft and false mergeability, preflight races, native policy rejection and no
strategy fallback. They check native/canonical result IDs and observable PR/
parent/base-tip verification, with unknown mergeability retained rather than
reported as true. Ignored SHA/racing head fixtures do not acquire atomic
guarantees. Failed verification, actual socket disconnects after saving and
whole-call deadlines retain target/result-commit evidence without replay.
An exact native-result parser unit test rejects arbitrary SHA substrings.

Existing API tests include wrapped 404 classification after merge preflight;
the longer merge request-budget regression still verifies the complete service.
Content tests verify exact message transmission without claiming REST exposes
the full stored message. MCP schemas reject removed `sha`, native-name aliases
and unknown arguments; read-only checks still cover all 16 mutations.

Against the dedicated disposable stock GitBucket setup below:

```bash
cargo build --locked
python3 scripts/e2e/probe_merges.py \
  --allow-disposable-writes --version-label 4.48.0 \
  --mcp-binary target/debug/gitbucket-mcp-server \
  --output /tmp/gitbucket-p12-observations
```

The probe requires explicit disposable writes and a root-context HTTP loopback
URL. It creates its own private repository and slash-containing base/head
branches, checks each two-commit strategy through actual stdio MCP, and compares
Git parent IDs, commit counts and merge/squash message suffixes. Its owner-only
Web setup enables all three strategies, then restricts the new fixture repo to
merge-commit and checks definite squash rejection without branch/PR mutation.
Unrelated settings fields are preserved. Five guard/message refusals plus policy,
draft and read-only rejection give eight negative cases. A separate raw native
PUT proves the wrong `sha` is ignored and records the exact stock result format.
Only safe observations are written; credentials, subprocesses and temporary Git
workspaces are released on failure. The dedicated Docker volume must be removed
afterward, as in the [Docker flow](#docker-e2e-flow).

All these checks and the existing 23 live E2Es passed on each official 4.44.0 and
4.48.0 image with the standard profile. Existing owner/GUEST review reads and
P10 posting checks also passed. Three offline Python checks validate form
preservation, rejection of malformed/ambiguous layouts and the explicit write
flag. Native blob provenance and the unverified message/rebase/atomic guarantees
are documented in [the merge contract](docs/pull-request-merges.md).

## List response pages (P11)

```bash
cargo test --test list_page_test
cargo test --test review_comment_test
```

Ten HTTP/MCP tests cover all seven REST list tools, unchanged array keys,
0/1/49/50/51/99/100/101 boundaries, default/maximum limits and complete identity
sets across continuations. They assert that unpaged sources are read once and
cached pages do not repeat upstream requests. Invalid limits, unissued/malformed
or foreign cursors, other targets/object numbers/filters/views/limits fail before
HTTP; replayed cursors are stable. Fresh observations see changed data while
continuations keep the collected values and report non-atomic consistency.

Huge Issue/PR content and repeated head/base repositories are omitted in
summaries with explicit metadata, while detail getters retain full data.
Comment previews are UTF-8-safe prefixes with truncation markers; full view
preserves the original Markdown. Escaping-heavy full comment lists hit the byte
bound before the count bound, and every serialized CallToolResult (including
both MCP representations) stays within 64 KiB. Later source failures do not
become partial successes. A separate Web/MCP test traverses review comments
without refetching and preserves partial coverage on every page.

Cache unit tests cover expiry, count/byte-pressure eviction, observation limits
and rejecting a later unpageable item before issuing any partial result/cursor.
Existing schema/annotations/read-only tests cover all 42 tools. Full metadata
fixture tests request `view: "full"`; getters and API list contracts are unchanged.

The two existing large-list live E2Es now explicitly follow MCP cursors, validate
page byte size/ranges/observation identity and compare every created identifier.
The Issue fixture also contains a large Japanese Markdown body: summary pages
omit it, detail get preserves it and full-list view rejects the oversized item.
Run against the dedicated disposable setup below:

```bash
cargo test --test e2e_test test_e2e_list_ -- --ignored --nocapture
```

Live validation passed all 23 existing E2Es on each official 4.44.0/4.48.0 image
under the standard profile. The review stdio probe also passed owner/GUEST reads
and P10 posting checks, using full-view continuation to compare observed HTML
models. The [paging contract](docs/list-continuation.md) defines default summary
fields, cache/expiry bounds and the limits of source completeness.

## Review comment reads (P09)

`tests/review_comment_test.rs` covers the real HTTP adapter and MCP tool dispatch:
list/get scope and missing lookups, context paths, profile/credentials, strict
IDs, PAT/cookie identity, wrong page/markup, head changes, actual 403/404,
redirect refusal, limits, read deadlines and read-only registration. Parser unit
tests check both captured DOM forms, ordinary-only/empty pages, duplicate IDs,
folded cards, positions, special paths, HTML/raw distinction and body-contained
metadata lookalikes. MCP mock tests cover the public trait dispatch as well.

For a live check, first build the server:

```bash
cargo build --locked
```

Use the disposable environment setup below, adding this option to the probe:

```bash
python3 scripts/e2e/probe_inline_comments.py \
  --allow-disposable-writes --version-label 4.44.0 \
  --output "${GITBUCKET_E2E_ROOT}-review-observations" \
  --mcp-binary target/debug/gitbucket-mcp-server
```

Repeat for 4.48.0 in its own Compose project. The optional stdio client starts
the built binary with an isolated config directory, standard profile and
read-only mode. It creates a separate disposable GUEST PAT without saving it.
At four conversation snapshots (initial, head addition, history replacement,
merged), both identities compare MCP results with the observed page and test
found/not-observed lookups. `mcp_checks` records eight successful identity/snapshot
checks per version. Credentials and MCP processes are cleaned up on failure too.
Default P08 runs omit this option and retain their research-only behavior.

Live P09 verification passed the eight stdio checks and all existing 23 E2Es on
each official 4.44.0/4.48.0 image with the standard profile. It does not establish
complete stored-history or raw-Markdown support; see the
[read contract](docs/review-comment-reads.md).

## Review comment posts (P10)

```bash
cargo test --test review_post_test
```

Eight HTTP/MCP integration tests cover the dedicated posting tool, pinned
base/head patch, exact old/new line forms and own raw-body verification. They
reject invalid/lossy inputs and atomic guards before HTTP, and reject stale PRs,
invalid paths/context lines and preflight races without posting. After posting,
definite 403 rejection is distinct from redirect/malformed/reused-ID ambiguity;
verification failures, head changes, MCP deadlines and actual TCP disconnects
retain any captured ID and field evidence without replay. Patch unit tests
cover quoted UTF-8 paths, multiple hunks, no-newline markers and unsupported or
malformed diffs. Schema/annotation and MCP/direct-handler read-only checks cover
the new tool along with the other 31 tools.

Using the same dedicated disposable setup below, build the binary and enable
both read and write MCP checks:

```bash
cargo build --locked
python3 scripts/e2e/probe_inline_comments.py \
  --allow-disposable-writes --version-label 4.48.0 \
  --output /tmp/gitbucket-p10-observations \
  --mcp-binary target/debug/gitbucket-mcp-server --verify-posts
```

`--verify-posts` requires `--mcp-binary`. It adds three verified comments: an owner
new-side line, a GUEST old-side line and an owner added UTF-8/quoted filename.
Eight negative requests cover atomic mode, outer body/path whitespace, both
sides, zero line, old commit, context line and absent path. A read-only call is
also blocked. The probe compares review ID sets around rejected calls and checks
that ordinary IssueComment IDs did not change. `mcp_post_checks` records the
results without credentials; subsequent head-update/history-replacement/merge
snapshots still validate the read tools for both accounts. Default probe behavior
is unchanged without this flag.

These checks and the existing 23 live E2Es passed on each official 4.44.0/4.48.0
image with the standard profile. They establish the bounded
[posting contract](docs/review-comment-posts.md), including its preflight race
window, rather than atomic head protection or complete saved PR association.

## Inline comment contract probe (P08)

`scripts/e2e/probe_inline_comments.py` is an explicit research experiment, not an
MCP tool or a default E2E. It creates a private repository, a GUEST account, 11
commit/file/line comments and Git history; it replaces its own fixture branch
using a force-with-lease and merges it. Use only a dedicated disposable stock
GitBucket environment with the bootstrap root account. The probe requires
`--allow-disposable-writes` and an HTTP loopback URL with root context path.

Run each version in its own Compose project and volume:

```bash
for version in 4.44.0 4.48.0; do
  (
    set -euo pipefail
    export GITBUCKET_E2E_IMAGE="ghcr.io/gitbucket/gitbucket:${version}"
    export GITBUCKET_E2E_COMPOSE_PROJECT="gitbucket-inline-${version//./}"
    export GITBUCKET_E2E_HTTP_PORT=18204
    export GITBUCKET_E2E_BASE_URL="http://127.0.0.1:${GITBUCKET_E2E_HTTP_PORT}"
    export GITBUCKET_E2E_ROOT="$(mktemp -d /tmp/gitbucket-inline.XXXXXXXX)"
    export GITBUCKET_E2E_ENV_FILE="${GITBUCKET_E2E_ROOT}/runtime.env"
    export GITBUCKET_E2E_USER=inline-probe-owner
    export GITBUCKET_E2E_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')"
    trap 'bash scripts/e2e/down.sh' EXIT
    bash scripts/e2e/bootstrap.sh
    docker image inspect "${GITBUCKET_E2E_IMAGE}" --format '{{json .RepoDigests}}'
    source "${GITBUCKET_E2E_ENV_FILE}"
    python3 scripts/e2e/probe_inline_comments.py \
      --allow-disposable-writes --version-label "${version}" \
      --output "${GITBUCKET_E2E_ROOT}-observations"
  )
done
```

Reserve the Compose project names for this experiment; teardown removes their
volumes. Observations remain in the sibling `<runtime>-observations` directory;
teardown removes the runtime directory and credentials. This probe does not touch the configured
repository's branches: it creates its own repository for each run. Do not point
the loopback URL at a tunnel to a business server.

Captured, normalized evidence is in `tests/fixtures/inline_comments/`. It records
image digests, exact upstream source blobs, request fields and statuses, body
loss, GUEST authorization, both DOM card forms, fold state, head relocation,
history replacement and post-merge coverage. The selected HTML fragments have
been parsed and reserialized; the fixture README defines their limits.
The [design](docs/inline-comment-design.md) separates source findings, live
observations, untested cases and proposed extensions.

Offline fixture/guard checks run in CI alongside the existing runtime tests:

```bash
python3 -m unittest discover -s scripts/e2e -p 'test_*.py'
```

The six probe checks require no server. They cover target restrictions, upstream
JSON forms, captured DOM kinds/IDs, raw text and authorization limits, history
coverage and source/image provenance. Default research-probe completion alone
does not validate the MCP tools; use the P09 option above. The normal Rust suite
still ignores its 23 existing live E2Es; the probe is a separate command.

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

For a fresh disposable 4.48.0 stack, set `GITBUCKET_E2E_IMAGE=ghcr.io/gitbucket/gitbucket:4.48.0` for bootstrap and explicitly set `GITBUCKET_E2E_API_PROFILE=standard` for the tests. Stop/remove any previous stack first or use separate Compose project, port and runtime-directory overrides. Image choice does not infer the profile; both tested versions use the same `standard` contract. The E2E client defaults to `rest-only` when no profile is supplied; tests requiring web operations need an appropriate explicit profile. The shared standard profile is tested against both versions with the full 23-case suite.

## GitHub Actions E2E

The [P14 acceptance policy](docs/acceptance.md) defines evidence layers, domain
selection, supported scope, safe reports and staged deployment checks.

- `.github/workflows/ci.yml` runs fmt, locked Clippy/tests, MSRV, shell syntax and
  all Python tests on pushes/PRs. PRs also select affected live domains on 4.48.0;
  docs-only changes skip Docker. `Required checks` aggregates required results.
- `.github/workflows/e2e.yml` runs the full matrix (4.44.0 and 4.48.0) nightly and
  on `workflow_dispatch`; failed entries do not cancel the other version.
- Release checks gate packaging/publication on stable/MSRV and that same full matrix.

All callers reuse `.github/workflows/contract-e2e.yml` and
`scripts/e2e/acceptance.py`, explicitly selecting the standard profile and
loopback official images. Full acceptance runs the 23 existing E2Es once plus
the actual stdio review/post/merge probes and PAT-only startup checks. Runtime
credentials stay private and are not imported into GitHub's shared env file.
The artifact contains only an allowlisted `summary.json`; raw logs, runtime
files, cookies and probe HTML/JSON are discarded. Cleanup runs in the runner
and again in an always-run workflow step. See the policy for local reproduction.

## Adding Tests

- Put API contract coverage in `tests/api_client_test.rs`.
- Put MCP protocol and tool-surface coverage in `tests/mcp_server_test.rs`.
- Keep tool-module unit tests near the implementation in `src/tools/*.rs`.
- Prefer write-path E2E only when setup is explicit and reruns are collision-safe; cleanup may be best-effort if tests use dedicated disposable data such as uniquely named repositories and branches.
- Prefer asserting structured MCP payloads (`structured_content`, `is_error`) instead of `"Error: ..."` text.
- Never hardcode real tokens or instance URLs in tests or fixtures.

## Ordinary comment operations (P15-2)

`cargo test --locked --test comment_item_test` covers raw Markdown, original
author after a developer edits another author's comment, HTML links,
ID and repository validation, rejection without fallback, exact stored-text
mismatch, post-write GET failures, ID-shaped error statuses, observed absence and lost repository visibility. The shared MCP tests check registration, strict schemas, annotations
and enforced read-only access including direct handlers.
Editing requires a matching conversation-comment GET before PATCH; preflight
401/403/404/500 and foreign IDs are tested to send no PATCH, while post-write
failures retain their operation evidence.

The core acceptance `changes` stage also calls
[`probe_comment_items.py`](scripts/e2e/probe_comment_items.py) using its fresh
private PR fixture. Actual stdio clients with PAT-only `rest-only` configuration
check Issue and PR comments, full raw text, owner/GUEST authors, GUEST own edits
and deletion, developer edits of another author's comment, denied writes to
others' comments, read-only rejection, wrong repository and repeated deletion.
It also uses the merged PR's timeline entry, confirms that native PATCH accepts
it despite its ordinary-comment GET returning 404, and verifies that the MCP
edit rejects that ID without changing the entry's body.
Both supported official Docker versions run these checks; private probe logs and
credentials follow the acceptance runner's disposal policy.

## Issue/PR label assignments (P15-3)

`cargo test --locked --test issue_label_test` checks scoped parent existence,
unpaged REST reads, add-only-missing requests, exact names and URI encoding,
replacement/clear, removal without deleting repository definitions, unchanged
no-ops, preservation mismatch, error outcomes without retry/fallback and cached
cursors bound to the Issue/PR and tool. Shared MCP tests cover required inputs,
strict schemas, annotations and enforced read-only access including handlers.

Core acceptance's `changes` stage calls
[`probe_issue_labels.py`](scripts/e2e/probe_issue_labels.py) in its disposable
private repository. Actual PAT-only rest-only stdio clients check Issue and PR
assignments, automatically created definitions and colors, exact Unicode/special
names, repeated add, replacement and clear, removal preserving definitions,
owner/GUEST permissions, checked no-ops, read-only rejection, missing-parent
protection and cached continuation over more than 50 labels while assignments
change. Both official supported versions run these checks with the existing
acceptance cleanup and private-log disposal policy.

## Creation metadata (P15-4a)

`cargo test --locked --test creation_metadata_test` covers existing/missing/
foreign references, exact body and names, normalized assignees, explicit draft
true/false and omitted flags, arrays missing versus empty, ignored metadata,
partial observations and created-ID retention after GET failure/identity mismatch
or MCP deadline. Unknown request fields and unsupported update fields are
rejected by strict schemas/models. No failure triggers another mutation.

The core `changes` stage invokes
[`probe_creation_metadata.py`](scripts/e2e/probe_creation_metadata.py) against
its disposable private repository. Actual PAT-only rest-only stdio clients check
open/closed milestones, two assignees and an exact existing label, invalid and
foreign references without extra Issues, GUEST creation whose assignment is
ignored but whose created number is reported, explicit empty metadata, draft
true/false/default, draft get/list observations, existing draft merge rejection
and read-only rejection. Existing acceptance policy discards credentials and raw
probe logs, retains only the allowed summary and cleans the dedicated Docker
environment. Both official supported versions run the probe.
