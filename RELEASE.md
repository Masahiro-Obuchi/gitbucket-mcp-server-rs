# Release Process

This project publishes GitHub Releases from version tags. The release workflow builds platform archives, attaches checksums, and publishes the GitHub Release.

## v0.3.0

This release exposes 53 MCP tools, including 22 reads. It adds repository
discovery and organization repository creation, Issue/PR assignments and
comment operations, fork PR creation, draft readiness, PR change reads and
inline comments. The standard GitBucket contracts are verified on 4.44.0 and
4.48.0. Source builds use Rust 1.88 or newer.

### Upgrading from v0.2.3

- Keep the existing URL and PAT configuration. For operations that use the Web
  UI, set `GITBUCKET_API_PROFILE=standard` and supply `GITBUCKET_USERNAME` and
  `GITBUCKET_PASSWORD` for the same account as the PAT. The default
  `rest-only` profile uses REST. See the [profile and route table](README.md#gitbucket-api-profiles).
- Refresh any cached MCP tool schemas. Remove unsupported arguments, such as
  label `description` or inline positions on a conversation comment. Use the
  dedicated tools and preserve the distinction between omitted metadata and
  explicit empty values. See the [input and model changes](#p07-input-and-model-changes).
- Traverse list results using `has_more` and `next_cursor`. The default returns
  up to 50 summary items per page. Request `view: "full"` or a detail getter
  for fields omitted from summaries. Treat missing `closed_at` and nullable
  metadata as unknown. See the [pagination contract](docs/list-continuation.md).
- Update error handling for `operation_unknown`, `operation_not_verified`,
  `operation_partial` and `operation_not_applied`. Inspect the target and
  recorded field results after an uncertain write before retrying. See
  [write outcomes](README.md#write-outcomes).
- PR merge callers can choose merge, squash or rebase. For an expected head,
  use `expected_head_sha` with `head_guard: "preflight"`. This check leaves a
  race window. Rust callers must migrate from the old `sha` field. See the
  [merge contract](docs/pull-request-merges.md).
- Custom Rust adapters must implement the current `GitBucketApi` methods and
  update struct literals, response types and exhaustive error matches. Replace
  `Config::from_env` with `Config::load` or `Config::load_with_file`. See the
  per-feature notes below.
- Run `--check-config` and `--check-connection` after upgrading. The stdio
  process reports readiness after the MCP initialization handshake. See
  [the installation checks](VERIFICATION.md).

### Pull request creation from an explicit fork

- Add `create_fork_pull_request` for an explicitly named source, including
  renamed forks.
- Use the standard Web compare form with matching PAT and Web identities.
  Check the comparison base against both branch histories and recheck both
  tips before one submission. Read back PR metadata and the fetched head ref.
- Custom `GitBucketApi` adapters add one required method with
  `CreateForkPullRequest` input. See [the contract](docs/fork-pull-request-creation.md).

### P15 organization repositories and discovery

- Add create_organization_repository for existing GitBucket organizations and
  list_visible_repositories for PAT-scoped discovery across user and organization
  owners.
- Organization creation verifies the owner, repository fields and initial
  branch state. Discovery uses a bounded owner scan and cached MCP pages.
  Both tools use REST in both profiles. See [the contract](docs/repository-discovery.md).
- Custom GitBucketApi adapters implement two new required methods using
  the existing CreateRepository and Repository models.

### P15 comment with close/reopen

- Add close_issue_with_comment and reopen_issue_with_comment for Issues and
  unmerged PRs. Both profiles use PAT-only REST; a real nonblank body is required and retained exactly.
- Send one native action-query POST and independently verify comment membership,
  body/author, parent state and retained fields. A denied action can still create
  a comment and return partial. Already requested states and merged PRs stop
  before writing. There is no retry, corrective write, fallback or rollback.
- Retain the created comment ID, parent and per-field observations across later
  failures, deadlines and cancellation. POST 404 is uncertain because native can
  return it after saving. Parent identity errors retain observed_parent_number.
- Custom GitBucketApi adapters implement two new required methods using existing
  CreateComment and new CommentStateChange. No compatibility defaults or new
  settings/dependencies. See [the contract](docs/comment-state.md).

### P15 pull request readiness

- Add mark_pull_request_ready for the native one-way draft-to-ready transition.
  The standard profile is required. Actual changes need matching PAT/Web credentials and head-repository developer
  permission. Already ready is a verified PAT-only no-op.
- Recheck content/revisions before one empty cookie-only POST; verify draft=false
  and preserved fields afterward. Retain target/head/evidence on uncertainty,
  without retry, redirect, reverse toggle, merge, rollback or atomic-head claims.
- Custom GitBucketApi adapters implement one new required method returning the
  existing PullRequest type. No compatibility defaults or new settings/models/
  dependencies. See [the contract](docs/pull-request-ready.md).

### P15 Issue/PR assignee updates

- Add add_issue_assignees, remove_issue_assignee and replace_issue_assignees for
  individual accounts on Issues and PRs.
- Require standard profile and matching PAT/Web credentials even for no-ops;
  reconcile complete REST/sidebar observations to protect hidden suspended-account
  relations. Add missing accounts before removing extras, verifying every step
  and preserved raw content/state/labels/milestone.
- Preserve target and per-login progress through partial failure/deadline/cancel;
  no retry, redirect, fallback or rollback. Custom GitBucketApi adapters implement
  three new required methods using AssigneeNames, without compatibility defaults.
  See [the contract](docs/issue-assignees.md).

### P15 Issue/PR milestone assignment updates

- Add set_issue_milestone and clear_issue_milestone for Issue/PR assignments.
  Milestone definitions are preserved. The standard profile is required; rest-only refuses before HTTP.
- Check target/reference identity, use one cookie-only Web POST and verify
  stored milestone plus retained raw content, state, labels and assignees.
  Clear requires the native explicit empty marker as well as the REST read.
- Keep target/field evidence on failure. POST 404 can follow a saved assignment
  and stays unknown; no replay, redirect, fallback or rollback is introduced.
- Custom GitBucketApi adapters must implement the two new required methods.
  See [the contract](docs/issue-milestones.md) and [tests](TESTING.md#issuepr-milestone-assignments-p15-4b).

### P15 creation metadata

- Add optional milestone to create_issue and draft to create_pull_request.
  Both profiles use PAT-only REST.
- Preflight supplied Issue reference names/IDs; verify supplied assignees,
  labels and milestone after one POST. Verify explicit PR draft true/false.
  Do not infer unobserved arrays/flags or retry a creation after verification fails.
- Capture the created number before verification. GitBucket can ignore Issue
  metadata for callers without developer permission after creating the Issue;
  failure evidence locates the existing resource when its number is known.
- Public Rust CreateIssue / CreatePullRequest literals must supply the new
  milestone / draft Option fields. Their JSON deserializers now reject unknown
  fields; unsupported PR creation metadata is not silently accepted.
- Core actual stdio acceptance covers open/closed milestone assignment,
  multiple assignees, ignored GUEST metadata, explicit empty arrays, draft
  true/false/default and get/list observations. See [the contract](docs/creation-metadata.md).

### P15 Issue/PR label assignments

- Add list_issue_labels, add_issue_labels, remove_issue_label and
  replace_issue_labels (PAT-only REST in both profiles).
- Add only missing assignments; verify the entire resulting set. Replacement
  removes omitted assignments and [] clears them all. Unknown names create
  repository definitions with generated colors; removal preserves definitions.
- Matching sets are checked no-ops. Actual changes require developer permission.
  Failures retain target/labels evidence and never trigger retry or rollback.
- Custom GitBucketApi adapters must implement four new required methods using
  LabelNames; the operation resource enum adds IssueLabels. No defaults/aliases
  or new dependencies/configuration are added.
- Core actual stdio acceptance checks Issue/PR assignments, owner/GUEST access,
  special names, replacement/clear and cached pages across assignment changes.
  See [the contract](docs/issue-labels.md).

### P15 ordinary comment operations

- Add `get_issue_comment`, `update_issue_comment` and `delete_issue_comment` for
  Issue and PR conversation comments. All three use PAT-only REST in both profiles.
- Edit verifies the complete stored body and returns the original author via
  GET, avoiding upstream PATCH's editor attribution. A preflight GET prevents
  PATCH from editing non-conversation timeline entries. Delete verifies preflight existence,
  post-write absence and repository visibility despite upstream ID-shaped HTTP
  statuses. Failures retain comment ID and field evidence.
- Custom `GitBucketApi` adapters must implement three new required methods.
  `UpdateComment` represents complete replacement text; no aliases/default shims
  or runtime configuration/dependencies are added.
- Core acceptance checks actual stdio owner/GUEST permissions, Issue/PR raw text,
  other-author edits and verified deletion on both supported GitBucket versions.
  See [the contract](docs/ordinary-comments.md).

### P15 PR change reads

- Add three read tools: list_pull_request_commits (PAT-only unpaged REST),
  list_pull_request_files and get_pull_request_diff (standard verified Web patch).
- Preserve stored base/head, exact commit messages, file paths/modes/statuses and
  patch text. Binary payload omissions and upstream rename limitations are explicit.
  Diff text uses bounded UTF-8 chunks and the existing cursor/cache limits.
- Public GitBucketApi adapters must implement the three new required methods;
  return the new models in models::pull_request_changes. There are no default
  compatibility shims. See [the contract](docs/pull-request-changes.md).
- Core acceptance now runs a real stdio/Git change-read probe in both supported
  GitBucket versions, including 28 commits, head advance, exact diff and merged PR.

### P14 regression and acceptance gates

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

### P13 configuration, CLI and minimum Rust

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

### P12 verified pull request merges

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

### P11 bounded list responses

- All twelve MCP lists default to summary pages of at most 50 items. Optional
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

### P10 review-comment posting

- Add `add_pull_request_review_comment` for one changed line on an open PR's
  exact current head under the standard profile. Read-only mode hides and rejects
  the posting tool.
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

### P09 review-comment reads

- Add read-only `list_pull_request_review_comments` and
  `get_pull_request_review_comment`. These tools require the standard profile and
  matching PAT/Web credentials.
- Review results distinguish commit/file/line comments and use `commit:<ID>`
  identities. Results contain HTML and explicit partial coverage; unknown raw
  text, original positions and timestamps remain null. Missing lookups return
  `not_observed`.
- Add public review model types and two required `GitBucketApi` methods. Custom
  Rust adapters must implement them. `GbMcpError::ReadConflict` and the MCP
  `read_conflict` kind report observed PR changes during a read. No legacy shim
  or invented HTTP status is provided. See [the contract](docs/review-comment-reads.md).

### P07 input and model changes

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
python3 -m unittest discover -s scripts/release -p 'test_*.py'
```

5. Confirm the CI workflow is green on `main`.

## Create a Release

From the release commit on `main`, create and push an annotated tag:

```bash
./scripts/release/create_tag.sh
```

The script derives the tag from the package version and requires local `main`
to match the fetched `origin/main`. Pushing the tag starts
`.github/workflows/release.yml`.

For a publishing run, GitHub Actions fetches the tag and `main` from
`origin`. It verifies that the tag matches the package version, points to the
commit being built, and is reachable from `origin/main`. The workflow checks
the live refs before packaging and before staging the assets. A release commit
can still pass when later commits have advanced `main`.

The workflow uploads assets to a draft release and fetches both live refs again
after the upload. A failed recheck leaves the release as a draft. If a rerun
finds a published release for that tag, it leaves its assets in place. The last
ref check and the GitHub publication request are separate operations, so a ref
can still change between them.

The workflow:

1. runs formatting, clippy, and tests;
2. validates all acceptance scripts and the minimum Rust version, then runs full
   Docker contract acceptance on official GitBucket 4.44.0 and 4.48.0;
3. verifies that the tag matches `Cargo.toml`;
4. builds release binaries for Linux, macOS Intel, macOS Apple Silicon, and Windows;
5. packages archives with the binary, license, installation and upgrade guides,
   contract documentation, and the sample Skill;
6. generates `.sha256` checksum files;
7. stages a GitHub Release with generated release notes, rechecks the refs, and
   publishes the draft.

Manual `workflow_dispatch` defaults to dry-run mode (`publish=false`), which runs checks and packaging and uploads workflow artifacts without publishing a GitHub Release.

If you run `workflow_dispatch` on a tag ref and set `publish=true`, the workflow also publishes the GitHub Release.

## Verify a Published Release

After the workflow finishes:

1. Open the GitHub Release page for the tag.
2. Confirm all expected archives and `.sha256` files are attached.
3. Download the archive for your platform.
4. Verify the checksum:

```bash
shasum -a 256 -c gitbucket-mcp-server-0.3.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

5. Follow `VERIFICATION.md` from the archive to confirm the binary starts and the MCP client can list tools.

## Publish the Cargo Crate

Publish to crates.io separately from the GitHub release. Check out the verified
release tag and run the package verification before publishing:

```bash
cargo publish --locked --dry-run
cargo publish --locked
```

## Failed Releases

For a transient failure, rerun the failed jobs on the existing tag. If a fix
requires a new commit, check that the failed tag's version matches the corrected
package version on `main` and that the version has not been published. Delete
that specific tag locally and on `origin`, then rerun the tag script from the
corrected `main`. Use a new patch version for a release already published on
GitHub or crates.io.
