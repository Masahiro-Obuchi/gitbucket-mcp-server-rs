# Verified pull request merges (P12)

`merge_pull_request` performs one REST PUT, then independently reads the PR,
result commit and target branch. It requires only the configured PAT; neither
profile enables Web fallback or strategy fallback. The repository's native
merge policy and permissions remain enforced by GitBucket. There are still
32 tools (16 reads); read-only mode hides/rejects merge before HTTP.

## Inputs and head guards

| Input | Contract |
| --- | --- |
| `owner`, `repo`, `pull_number` | Existing repository rules and a positive PR number |
| `merge_method` | Optional `merge` (default), `squash`, `rebase` |
| `commit_message` | Optional original text appended by native merge/squash formatting; unsupported for rebase, which ignores it |
| `expected_head_sha` | Optional full lowercase 40-character SHA-1; requires explicit `head_guard: "preflight"` |
| `head_guard` | Optional `none` (default), `preflight`, `atomic` |

The ordinary default is `head_guard: "none"`, without an expected SHA. Its
state/read consistency checks do not assert that a previously reviewed head was
selected. To check a reviewed SHA, explicitly provide both `preflight` and
`expected_head_sha`. Missing/mismatched combinations and malformed SHAs are
input errors before HTTP. `atomic` always returns `unsupported_error` before
HTTP under the verified contract, including the rest-only profile.

GitBucket 4.44.0/4.48.0 ignore the REST `sha` parameter. The service never sends
it as a supposed condition. There is a race window between the final metadata
read and the PUT; before/after observations cannot make that condition atomic.
Atomic head guarantees require server-side support that this adapter does not
implement. The old Rust request `sha` field is removed, not aliased.

`merge` maps to the native `merge-commit` strategy; squash/rebase retain their
native names. Clients do not supply `merge-commit` as an alias. Repository
options accept those native names, but the REST repository model does not expose
the allowlist. The service does not require owner-only Web settings access or
invent a local policy check. A prohibited strategy's native 405 is a definite
rejection, with no alternate strategy or Web write attempted. The generic API
error does not distinguish policy, hooks and every other rejection reason.

Rebase rejects any supplied `commit_message`, including an empty string, before
HTTP instead of silently dropping it. Merge/squash send the supplied text
unchanged; GitBucket adds its own message prefix. REST commit details expose
only the short message, so the service does not claim a full-message readback.
Unknown arguments, including `commit_title` and legacy `sha`, are rejected.

## Preparation and one write

1. Validate inputs, GET the exact PR and require open state, `merged: false`
   and `draft: false`. True or unknown draft state is unsupported before a
   mutation. `mergeable: false` rejects the request; null remains unknown and
   GitBucket performs its native conflict check when handling the PUT.
2. Require canonical PR head/base SHA metadata and compare an explicitly
   expected head. Read the exact target branch to record its current tip.
3. GET PR metadata again and require an unchanged snapshot. An observed change
   is `read_conflict`, with no mutation. After this final read, send exactly one
   PUT to `/repos/<owner>/<repo>/pulls/<number>/merge` with native strategy and
   optional message. The existing explicit merge HTTP timeout still applies.

The PUT follows no redirects and performs no automatic retries, alternate methods,
branch rewinds or rollback merges. Native conflict/policy validation remains authoritative;
null mergeability is never reported as true by the client.

## Result observations

Success requires a positive merge response, a canonical result commit ID, an
independent PR read with the same identity/revisions/base branch and
closed/merged state, a GET of that result commit and a GET of the target branch
whose tip equals the result ID. Branch names with `/` are encoded as data and
were exercised on both versions. A concurrently advanced base tip causes an
unverified/partial outcome rather than a different success SHA.

Parent checks are scoped to observable shape:

| Requested method | Required parent observation |
| --- | --- |
| merge | Exactly the recorded base tip followed by the observed PR head |
| squash | Exactly one parent, the recorded base tip |
| rebase | Exactly one canonical parent; full replay/tree equivalence is not established |

`MergeResult` returns canonical string `sha`, `merged: true`, optional upstream
status `message`, `requested_merge_method` and `verification`. The method field
states the request, not independent proof of an entire strategy execution.
`verification` reports the selected `head_guard`, `atomic_head_guard: false`,
optional `expected_head_sha`, observed head, base tip before writing and the
nullable `mergeable_before`. `pr_state_verified`, `base_ref_verified` and
`commit_parents_verified` are true on success. `merged_head_verified` is true
only for merge-commit's explicit second parent; it remains false for
squash/rebase. `commit_message_verified` and
`repository_policy_preflight_verified` remain false. These sequential reads do
not prove atomicity, full replay equivalence or causal ownership against another
simultaneous merge.

Native responses need normalization before they can be used as commit IDs:
4.44.0 returns `AnyObjectId[<SHA>]`; 4.48.0 returns
`MergeResult(AnyObjectId[<SHA>],List(<SHA>, ...))`. The parser accepts those exact
known frames or an already canonical SHA, validates all IDs and rejects arbitrary
substring extraction. The normalized result is then checked independently;
native response text alone is not verification. Unknown formats remain errors.

The upstream `GET /pulls/<number>/merge` is deliberately unused: on these versions
its implementation checks conflict state, not whether the PR is merged. Likewise,
the commit model's `tree.sha` contains a commit ID and is not tree-hash proof.

## Failures and reconciliation

Before the PUT, validation/authentication/unsupported/conflict errors keep their
original kinds. HTTP or validated native-wrapper 4xx rejection except 408 is `operation_not_applied`.
Once a PUT may have applied, redirects, malformed/lost responses, 408/5xx,
failed verification or interruption produce existing structured unknown/partial/
unverified outcomes. A failed follow-up 403/404 does not prove no merge occurred.

`operation.target` retains owner/repo/PR number, target branch in `name`, observed
pre-write head in `commit`, and `result_commit` as soon as a canonical result is
captured. The unchanged PR head has preserved intent; observing it alone never
counts as partial application of a merge. Captured result IDs and completed field observations survive later
deadline/cancellation. Responses lost before a complete result may have no
result ID. Inspect the PR/base branch and any captured commit before deciding
on another action; the service never replays a merge automatically.

## Provenance and validation

Official Git blobs used to establish the contract:

| Source | 4.44.0 | 4.48.0 |
| --- | --- | --- |
| [ApiPullRequestControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala) | `f2529bb2a8acb93be8e423866322ada649c2d148` | `112460b0d7a96e006f218250f74a350ffcffc199` |
| [MergeAPullRequest.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/MergeAPullRequest.scala) | `0ae4d10727d717696b0e6585fa9a160ff5fb79ba` | same |
| [MergeService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/MergeService.scala) | `1e76379404d500a99a79592680108dd76196cb8e` | `fc721af4832ae0308cc13771dde63248690e817c` |

Additional 4.48.0 sources: repository options validation
[`9d3c0eb48df6be30662606c069456a3e5ef2fa11`](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/RepositorySettingsController.scala),
repository model without a merge allowlist
[`5b623547405990410238c1ab95d4d4cab4758390`](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiRepository.scala),
and commit short-message/tree metadata
[`49777aa069fd1fb2bc82c99e13757e5bd665f84c`](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiCommits.scala).

Eight HTTP/MCP tests cover strategies, policy, draft/unknown mergeability,
preflight races, ignored SHA conditions, retained identity, no replay, actual
saved-response socket disconnects and whole-call deadlines. The native parser
has its own unit test. Disposable stdio/Git checks on stock 4.44.0/4.48.0 verify
three strategies, eight rejections, actual history/message suffixes, branch names
with `/`, and the ignored raw REST SHA. All existing 23 E2Es passed per version.
See [reproduction instructions](../TESTING.md#pull-request-merges-p12).
