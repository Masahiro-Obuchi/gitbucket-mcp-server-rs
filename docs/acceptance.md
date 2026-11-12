# Regression and release acceptance

This is the P14 acceptance policy for the behavior implemented through P13.
It adds automation, not production tool capabilities or a new release tag.

## Evidence layers

| Layer | Responsibility | Limit |
| --- | --- | --- |
| Normal Rust tests | REST/Web request contracts, models, outcomes, budgets, paging, schemas and handler enforcement using synthetic fixtures/mocks | Does not establish compatibility with a live GitBucket |
| CLI/stdio subprocess tests | Configuration resolution, diagnostics, initialization and shared private-CA trust against local HTTP/TLS servers | Does not test a real GitBucket deployment |
| Python infrastructure tests | Domain selection, private runtime handling, failure/timeout cleanup, safe report generation and probe parsers | Does not establish live behavior |
| Docker contract acceptance | Actual official GitBucket behavior through REST, Git, Web and the built binary's stdio MCP tools | Only the tested images, routes and scenarios below |

Run `cargo fmt --all --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked`, and Python discovery for each PR. Rust 1.88.0 performs a
locked build and all-targets/all-features check on Linux. A mock-only result must
not be described as successful GitBucket compatibility.

## CI and supported matrix

| Trigger | Live contracts | Required checks |
| --- | --- | --- |
| Branch push | None; the PR supplies domain contracts | Stable checks and MSRV |
| Pull request | Selected domains on official 4.48.0 after fast checks pass | Stable, MSRV, valid plan and every selected domain |
| Nightly or manual E2E | All domains on official 4.44.0 and 4.48.0, independent jobs | Both matrix entries |
| Release tag or release dry run | All domains on official 4.44.0 and 4.48.0 | Stable checks, script tests, MSRV and both live entries before packaging/publication |

The PR's `Required checks` job aggregates success and fails for a failed,
cancelled or unexpectedly skipped prerequisite. It permits live contracts to be
skipped only for a successfully computed docs-only plan. Repository maintainers
can use this stable job name for branch protection; this change does not alter
repository protection settings. The shared workflow is
[`contract-e2e.yml`](../.github/workflows/contract-e2e.yml).

The [selector](../scripts/e2e/ci_plan.py) compares PR base/head commit IDs using
NUL-delimited Git paths with rename detection disabled. Deleted and renamed
source paths remain visible. Documentation/Skill-only changes skip live tests.
Changes to CI, harnesses, Docker, dependencies, shared/unknown code run all
domains. Feature files select the union of affected domains; shared PR tools
include core/paging/review/merge, and common list projection includes paging and
review. This conservative mapping is tested and must be updated when adding
domains or moving implementation files. Fast tests always run.

| Domain | Live checks |
| --- | --- |
| `core` | Existing E2E target excluding the two large-list tests: repository/branch/user, labels, milestones, Issue/PR writes, retained fields, exact content, read-only enforcement; includes explicit PAT-only milestone and standard Web paths, plus the actual stdio PR-change probe (28 commits, head advance, files/diff and merged PR) |
| `paging` | List E2Es, including 26+ Issue/PR and 101 unpaged resources, MCP cursor traversal, explicit summary omission and full/detail content |
| `review` | Existing inline probe with actual stdio owner/GUEST reads across initial/head-add/history-rewrite/post-merge snapshots, three verified MCP posts and rejection paths |
| `merge` | Existing merge probe: merge/squash/rebase, independent Git parent/tip checks, policy/draft/head/read-only rejection and native ignored-SHA behavior |
| `startup` | Full env with unused malformed TOML, local/explicit PAT diagnostics, actual PAT-only stdio read and blocked write |

When core and paging are selected together, all existing E2Es run once. A
successful cargo command with zero executed tests is rejected. Paging must
observe successful execution of both named large-list tests. Review and merge
contracts retain the limits in [review reads](review-comment-reads.md),
[review posts](review-comment-posts.md) and [merges](pull-request-merges.md).

Both supported images use the same explicit `standard` profile, bundled plugins
only, `/` context and loopback HTTP. The runner records the resolved official
image digest and image ID. This does not cover custom plugins, SSO/MFA, reverse
proxy/context configurations, arbitrary future versions or platform MSRV builds.
Private-CA behavior is covered separately by the local TLS subprocess tests.

## Reproduce and inspect evidence

From the repository root, with Docker, Git, Python 3 and stable Rust installed:

```bash
python3 -m unittest discover -s scripts/e2e -p 'test_*.py'
python3 scripts/e2e/acceptance.py --allow-disposable-writes \
  --version 4.48.0 \
  --runtime-root /tmp/gitbucket-acceptance-runtime-448 \
  --output /tmp/gitbucket-acceptance-report-448
```

Repeat with `--version 4.44.0` and separate runtime/output paths. Runtime paths
must be new and outside the repository, output and Cargo target directory, since
cleanup removes the runtime directory. `--port`, `--project` and `--target-dir`
allow isolated local runs. The default Compose project contains a random suffix.
Do not reuse a project belonging to another Docker stack. To run only one
changed area, add e.g. `--domains-json '["merge"]'`.

The runner creates its own disposable account/repositories and ignores external
GitBucket URL/token/image overrides. Opt-in is required because these contracts
write data. Its `finally` cleanup removes the dedicated container, volume and
runtime credentials. After each stage, including failure, timeout and cancellation,
the runner terminates the entire child process group even if its leader has
already exited. Members still running after a five-second SIGTERM grace receive
SIGKILL. The workflow also has an unconditional teardown step as a second attempt.
A cleanup failure fails acceptance. Hard termination of the runner/host can
prevent local cleanup; use `down.sh` with the same project/root if needed.

Only `summary.json` is published as a workflow artifact (14-day retention).
It contains a fixed schema, version/domains, source commit and dirty-worktree
flag, official image identity, transport/context/plugin scope, stage pass/fail,
timings and executed E2E count. It never copies subprocess output, exception
text, upstream payloads, instance URL, account identity or credentials. Reports
from a dirty worktree identify the base commit but do not claim it describes all
tested code; CI checks out a clean commit.

Raw child logs, runtime JSON/shell files, cookies and probe observations remain
in private temporary directories and are discarded on success and failure.
They are not uploaded, and raw diagnostics are not printed when a stage fails.
Use the failed stage name to reproduce the relevant commands from
[TESTING.md](../TESTING.md) in a local disposable environment. Release asset
downloads select `release-*` artifacts, so contract reports are not bundled into
the distributed binary archives.

## Staged deployment acceptance

CI does not access a business GitBucket. Before an authorized deployment:

1. Review migrations in [RELEASE.md](../RELEASE.md), the supported matrix and the
   exact source/digest evidence. Use a candidate build that passed the gates.
2. In an isolated repository, run `--check-config`, explicitly run
   `--check-connection`, then use a read-only MCP client for identity, repository,
   Issue/PR/list and relevant review reads. Diagnose Web credentials separately;
   the connection command does not submit the password or prove Web access.
3. With explicit authorization, use disposable Issue/PR fixtures for the specific
   update/post/merge operations needed, and inspect their structured outcomes and
   readback. Partial/unknown results require inspection, not automatic retries.
4. Record the instance/version/plugin/context/CA scope privately and remove test
   fixtures. Only then perform the separately authorized deployment/release.

If rollback would restore a known data-loss path, disable the affected write
capability rather than reintroducing that behavior. Partial review observations
and non-atomic preflight merge guards remain limitations, including after a
successful acceptance run.

The core `changes` probe also verifies ordinary Issue/PR comment get/edit/delete
through actual PAT-only stdio clients. It checks owner/GUEST permissions, exact
Markdown and original authors after editing, repository scope, read-only
rejection and deletion with a subsequent missing lookup. See the
[ordinary comment contract](ordinary-comments.md).

The core `changes` stage also exercises Issue/PR label assignment list/add/remove/
replace over actual PAT-only stdio, including unknown-name repository creation,
replacement clearing, permission rejection, checked no-ops and cached pages over
55 labels. See the [assignment contract](issue-labels.md).
