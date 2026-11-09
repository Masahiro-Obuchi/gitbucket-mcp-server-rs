# Release Process

This project publishes GitHub Releases from version tags. The release workflow builds platform archives, attaches checksums, and publishes the GitHub Release.

## Unreleased P14 regression and acceptance gates

- PR changes select live contract domains on official GitBucket 4.48.0 after
  fast checks; docs-only changes skip live checks. Unknown/shared code and CI/
  harness/dependency changes select all domains. `Required checks` aggregates
  stable/MSRV and required live results for a stable branch-protection target.
- Nightly/manual and release runs test official 4.44.0 and 4.48.0 through the
  shared disposable runner. Existing E2Es, stdio review/post/merge probes and
  PAT-only startup checks run without external deployment credentials.
- Release packaging/publication requires stable checks, script tests, Rust
  1.88.0 locked checks and both full matrix entries. No release is created by
  this implementation. Safe summary artifacts contain source/image identity and
  stage results; raw diagnostics/runtime credentials are never uploaded.
- Build Apple Silicon archives on `macos-15`; the previous `macos-14` runner
  was retired by GitHub on November 2, 2026 and cannot start jobs. See the
  [official retirement notice](https://github.com/actions/runner-images/issues/13518).
- See [the acceptance policy](docs/acceptance.md) for reproduction, scope limits,
  cleanup behavior and separately authorized staged production acceptance.

## Unreleased P13 configuration, CLI and minimum Rust

- Raise `rust-version` and source-build documentation to 1.88, the minimum
  required by the existing locked dependency graph. Linux CI builds/checks with
  1.88.0 and tests/lints on stable, using `--locked`. Production dependency
  versions are unchanged; TLS process tests add an already-locked dev dependency.
- Full URL/token environment configuration skips the config path and TOML file.
  Partial env still fills missing values from TOML and fails on invalid used
  files. Blank/invalid-UTF-8 environment credentials fail without fallback.
  Web credentials remain env-only and paired, and Unix atomic `0600` saves and
  file protections are preserved. Rust `Config::from_env` is removed; migrate
  to `Config::load` or `load_with_file` with the same resolution rules.
- Add configuration-free help/version, local `--check-config`, and explicit
  `--check-connection` (PAT identity GET only, no password sign-in). Success
  diagnostics are JSON without secrets, identity or private URLs. MCP readiness
  now follows the completed initialize/initialized handshake; direct launch
  waits for an MCP client. See [VERIFICATION.md](VERIFICATION.md).
- Explicitly use Rustls across REST and Web clients. Install private CA roots in
  the platform store or supply `SSL_CERT_FILE`/`SSL_CERT_DIR`; these overrides
  replace platform root discovery. TLS verification remains required by the
  production CLI. See [README.md](README.md#https-and-custom-certificate-authorities).

## Unreleased P12 verified pull request merges

- Expose `merge_method` (`merge` default / `squash` / `rebase`) and explicit
  `head_guard` (`none` default / `preflight` / unsupported `atomic`). A reviewed
  `expected_head_sha` requires preflight; atomic requests fail before HTTP.
  GitBucket's ignored `sha` is not sent or treated as an atomic condition.
- Check PR identity/state/draft, observed revisions and base tip before one PUT.
  Verify PR closed/merged state, canonical result commit/parent shape and target
  tip afterward. Unknown mergeability stays null. Policy rejection is preserved
  without another strategy; rebase rejects ignored custom messages before HTTP.
- Normalize stock 4.44.0 ObjectId / 4.48.0 MergeResult string forms into a
  canonical SHA only with exact grammar and independent reads. `MergeResult`
  now has required string `sha`, boolean `merged`, `requested_merge_method` and
  `MergeVerification`. It describes observable results, not an atomic guard,
  complete rebase equivalence or full commit-message readback.
- Rust `MergePullRequest` replaces `sha` with `expected_head_sha`, typed
  `MergeMethod` and `MergeHeadGuard`; update struct literals/custom adapters.
  `OperationTarget` gains optional `result_commit`; retained across interrupted
  verification. No legacy aliases, automatic retries or rollback are added.
  See [the contract](docs/pull-request-merges.md).

## Unreleased P11 bounded list responses

- All eight MCP lists now default to summary pages of at most 50 items. Optional
  `limit` accepts 1..100; `cursor` continues the same observation; `view: "full"`
  requests all existing model fields within limits. Array keys remain, with
  `has_more`, `next_cursor`, `range` and per-summary omission/truncation metadata.
  Clients must traverse pages; one call or `limit: 100` no longer means all items.
- Responses are capped at 64 KiB including both MCP representations. Oversized
  full items/observations fail explicitly. Review coverage remains partial on
  the last page. Initial traversal and continuations are not atomic snapshots.
- Cursors are server-issued, bound to normalized scope/filters/view/limit and
  expire after five minutes, earlier on cache pressure/restart. Invalid cursors
  fail before HTTP; cached pages never silently refetch a different observation.
- Rust list Params gain `limit`, `cursor`, `view`; update struct literals. Add
  public `ListOptions` / `ListView`; remove the unused unbounded `success_list`
  helper. API/adapter Vec methods and detail models remain unchanged. No legacy
  mode or environment setting is added. See [the contract](docs/list-continuation.md).

## Unreleased P10 review-comment posting

- Add `add_pull_request_review_comment` for one changed line on an open PR's
  exact current head under the standard profile. The server advertises 32 tools,
  including 16 reads. Read-only mode hides and rejects all 16 writes.
- Require explicit `head_guard: "preflight"`, accepting its race window;
  `atomic` is unsupported before HTTP. Lossy outer body/path whitespace, control
  characters, historical/context/file-wide positions and unsupported diffs are
  rejected. No automatic retry, ordinary-comment fallback or rollback occurs.
- Verify the created ID's own raw body, commit-page position and PR-page
  observation before success. Unknown outcomes preserve the target and any
  captured ID. Stored PR association and full history remain unverified. See
  [the posting contract](docs/review-comment-posts.md).
- Rust adapters must implement the new required `GitBucketApi` posting method.
  Add public `CreateReviewComment`, `HeadGuard`, `PostedReviewComment` and
  `ReviewPostVerification` models, `ReviewBodyFormat::Markdown` and
  `ResourceKind::PullRequestReviewComment`. `OperationTarget` gains optional
  commit/path/old_line/new_line reconciliation fields, omitted when absent.
  Update exhaustive matches and struct literals; no compatibility shim is added.

## Unreleased P09 review-comment reads

- Add read-only `list_pull_request_review_comments` and
  `get_pull_request_review_comment`. The server now advertises 31 tools, including
  16 reads. The tools require the standard profile and matching PAT/Web credentials.
- Review results distinguish commit/file/line comments and use `commit:<ID>`
  identities. Results contain HTML and explicit partial coverage; unknown raw
  text, original positions and timestamps remain null. Missing lookups return
  `not_observed` rather than asserting deletion. No inline write tool is added.
- Add public review model types and two required `GitBucketApi` methods. Custom
  Rust adapters must implement them. `GbMcpError::ReadConflict` and the MCP
  `read_conflict` kind report observed PR changes during a read. No legacy shim
  or invented HTTP status is provided. See [the contract](docs/review-comment-reads.md).

## Unreleased P07 input and model changes

- All MCP tools reject unknown arguments before HTTP. Clients must remove
  unsupported fields, including inline locations on ordinary comment tools.
- Bodies/comments, descriptions and merge messages retain original whitespace.
  Empty text still clears optional update content; whitespace is not a substitute
  for an empty string. Standard web Issue updates that cannot preserve a changed
  body return `unsupported_error` before writes, including accompanying changes.
- Missing Issue `closed_at` now returns null, replacing the old updated_at-based
  estimate. Consumers must treat it as unknown and avoid closure-time calculations
  from updated_at.
- Issue and PR labels/assignees and repository fork are optional Rust fields and
  serialize as null when absent. Explicit empty arrays and false remain explicit.
  PR draft/merged_by/assignee/URLs/branch users and Issue milestone/assignee/URLs/PR
  links are retained. Do not interpret null mergeable or draft as false.
- `GITBUCKET_READ_ONLY=true` is an env-only MCP-server setting, default false.
  It hides mutations and rejects write calls with `read_only_error`, including
  public Rust tool handlers. Rust servers use `with_access_mode(AccessMode::ReadOnly)`.
- Every tool now carries behavioral annotations. They are advisory; read-only
  enforcement is independent, and writes are never automatically retried.

These changes intentionally correct the protocol and public response models;
there are no legacy aliases or synthetic timestamp/boolean fallbacks. Include
these migration notes in the next release. No version tag is created by this work.

## Versioning

Use semantic versions in `Cargo.toml`.

- Stable release tags must be `vX.Y.Z`, for example `v0.2.0`.
- Pre-release tags may include a suffix, for example `v0.2.0-rc.1`.
- The tag must exactly match the package version in `Cargo.toml` with a leading `v`.

## Pre-Release Checklist

Before tagging:

1. Confirm the intended user-visible changes are merged to `main`.
2. Update `Cargo.toml` and `Cargo.lock` to the release version.
3. Update README examples that mention a specific release tag.
4. Run local checks:

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
python3 -m unittest discover -s scripts/e2e -p 'test_*.py'
```

5. Confirm the CI workflow is green on `main`.

## Create a Release

Create and push an annotated tag from the release commit on `main`:

```bash
./scripts/release/create_tag.sh v0.2.0
```

Pushing the tag starts `.github/workflows/release.yml`.

The workflow:

1. runs formatting, clippy, and tests;
2. validates all acceptance scripts and the minimum Rust version, then runs full
   Docker contract acceptance on official GitBucket 4.44.0 and 4.48.0;
3. verifies that the tag matches `Cargo.toml`;
4. builds release binaries for Linux, macOS Intel, macOS Apple Silicon, and Windows;
5. packages archives with `README.md`, `LICENSE`, and `VERIFICATION.md`;
6. generates `.sha256` checksum files;
7. publishes a GitHub Release with generated release notes.

Manual `workflow_dispatch` defaults to dry-run mode (`publish=false`), which runs checks and packaging and uploads workflow artifacts without publishing a GitHub Release.

If you run `workflow_dispatch` on a tag ref and set `publish=true`, the workflow also publishes the GitHub Release.

## Verify a Published Release

After the workflow finishes:

1. Open the GitHub Release page for the tag.
2. Confirm all expected archives and `.sha256` files are attached.
3. Download the archive for your platform.
4. Verify the checksum:

```bash
shasum -a 256 -c gitbucket-mcp-server-0.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

5. Follow `VERIFICATION.md` from the archive to confirm the binary starts and the MCP client can list tools.

## Failed Releases

If the release workflow fails before publishing, fix the problem on `main`, delete the local and remote tag, then create the tag again from the corrected commit:

```bash
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
./scripts/release/create_tag.sh v0.2.0
```

If a GitHub Release was already published, prefer creating a new patch version instead of replacing released artifacts.
