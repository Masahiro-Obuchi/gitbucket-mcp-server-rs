# Review-comment posts (P10)

`add_pull_request_review_comment` creates one line review under the explicitly
selected standard profile. It uses GitBucket's native cookie-authenticated form
once, with an independent diff preflight and post-write observations. Ordinary
`add_pull_request_comment` remains a conversation comment. The server advertises
32 tools (16 reads and 16 writes); read-only mode hides and rejects this tool.

## Input and preservation

| Input | Contract |
| --- | --- |
| `owner`, `repo`, `pull_number` | Existing repository identifier rules and a positive PR number |
| `body` | Nonblank Markdown preserved exactly within the Web constraints below |
| `path` | Exact repository-relative Git path; no absolute, empty, `.` or `..` segments |
| `commit` | Full lowercase 40-character SHA-1 equal to the open PR's current head |
| `old_line`, `new_line` | Exactly one positive integer, at most 2147483647; select a deleted or added line respectively |
| `head_guard` | Required enum: `preflight` or `atomic`; only explicit `preflight` is supported |

Unknown arguments are rejected. No input text is silently rewritten. The native
form trims both body and filename, so leading/trailing whitespace in either is
`unsupported_error` before HTTP, even if both padded and unpadded files exist.
Body controls other than LF/tab, and all path controls, are also unsupported.
This excludes CR/CRLF and NUL. Internal Unicode, spaces, LF, tabs, code indentation
and Markdown hard breaks are preserved and compared with the saved raw text.
Whitespace-only bodies and invalid identifiers/positions are local input errors.

Configure `GITBUCKET_API_PROFILE=standard` with matching PAT,
`GITBUCKET_USERNAME` and `GITBUCKET_PASSWORD`. PAT and cookie identities must be
the same. Stock GitBucket permits a logged-in repository reader, including a
GUEST collaborator, to comment and read its own editable raw body; elevated
permissions are not requested. Other deployments may reject the native write.
The default rest-only profile does not probe speculative REST endpoints or
implicitly enable Web posting.

## Diff and head checks

1. Read PR metadata; require an open PR and the exact supplied head SHA.
2. Verify the Web session and GET `patch/<base>...<head>` pinned to the API's
   immutable base/head IDs. On the tested GitBucket versions this is a direct
   tree comparison, matching the PR comparison's endpoints; it is not a
   merge-base comparison despite the URL spelling. This full patch avoids the
   Web diff view's file/line display caps. The configured HTML byte limit still
   bounds the response; an oversized response aborts before writing.
3. Parse the unified patch, including quoted UTF-8 Git paths, hunk counts,
   offsets, multiple hunks and no-newline markers. Require an unambiguous changed
   line in an added/modified ordinary text file (100644/100755). The selected
   row becomes the native display diff JSON; it is not treated as server proof.
4. Read the PR conversation and retain the observed ID set. Check the cookie
   identity, then compare final REST PR metadata with the initial snapshot.
   Only after a match send the one native POST, with no intervening HTTP request.

Context-only lines, whole-file deletion, explicit rename/copy diffs, binary
files, symlinks, submodules, ambiguous/malformed patches, historical commits,
file-wide comments and closed/merged PRs are unsupported. Some of these can be
detected only after read requests. No write is sent on failed preflight.

`head_guard: "preflight"` explicitly accepts the race window between that last
metadata read and the POST. Neither a second read nor post-write verification
makes the operation atomic. `head_guard: "atomic"` returns `unsupported_error`
before HTTP. A deployment requiring atomic expected-head protection needs the
server-side extension discussed in [P08](inline-comment-design.md).

## One write and scoped verification

The only resource mutation is an AJAX POST to
`commit/<head>/comment/_data/new`, with `content`, `fileName`, `issueId`, one
line field and the selected display diff JSON. No redirects, automatic retries,
ordinary-comment fallbacks, edits or rollback deletes are performed.

After HTTP 200, capture a canonical new `discussion_r<ID>` immediately, rejecting
an ID already observed before posting. Before returning success:

1. Check the returned card's author, exact path and selected side/line.
2. GET the new comment's own edit fragment and compare its textarea's exact raw
   Markdown with the input. This GET does not edit the comment. Other authors'
   raw bodies are not requested by the posting tool or the list/get tools.
3. GET the immutable commit page; require its full commit identity and the same
   ID, author, path and side/line in its comments, including hidden line notes.
4. Require the same card in the PR conversation and recheck open state and
   base/head revisions. A changed `updated_at` alone is allowed here because the
   new comment itself can update it.

Success returns `PostedReviewComment` with `comment`, `coverage`, `verification`.
The comment contains `id: "commit:<ID>"`, numeric `storage_id`, rendered
`body_html`, verified `body_raw`, `body_format: "markdown"`, requested head in
`current_commit` and `position_verified: true`. That flag concerns this new
comment at the pinned requested diff only. Unknown original fields and exact
timestamps remain null; no historical metadata is reconstructed.

`verification` reports `head_guard: "preflight"`, `atomic_head_guard: false`,
`raw_body_verified: true`, `commit_page_verified: true`,
`pr_page_membership_verified: true`, `stored_pr_association_verified: false`.
PR-page membership is an observation, not proof of the saved row's `issueId`.
Global `coverage` retains P09's partial/history/raw-body/position limitations;
those aggregate flags are not upgraded by verifying one new comment. A later
list/get still returns HTML-only observed attributes under the
[read contract](review-comment-reads.md). Observations are sequential and can
change after verification.

## Failures and reconciliation

Local preflight errors retain their input/authentication/unsupported/conflict
kind without pretending a write was attempted. Once the POST starts, existing
operation outcomes distinguish definite HTTP rejection from unknown or partial
results. Actual rejecting 4xx headers, except 408, are `operation_not_applied`;
timeouts, disconnects, 408/5xx, redirects and malformed/oversized responses may
mean the comment was saved. A failed post-write GET, including 401/403/404, is
not evidence that creation failed.

`operation.target` retains resource `pull_request_review_comment`, owner/repo,
parent PR number, commit/path/selected side, and the numeric created ID as
`number` once captured. A reused preflight ID is retained as a candidate instead
of being called new. Captured identity and completed field observations survive
whole-call deadline/cancellation during later verification. Body and credentials
are not included in diagnostics. A disconnect before a complete creation
response can leave the ID unknown. Inspect the target and any captured ID before
manually deciding whether to retry; there is no idempotency guarantee.

## Provenance and validation

The native create/own-raw routes and trimmed form fields were checked against
official `RepositoryViewerController.scala`; the pinned patch against
`JGitUtil.scala` and `PullRequestService.scala`. The commit/edit markers come
from the official templates. Git blob identities:

| Source | 4.44.0 blob | 4.48.0 blob |
| --- | --- | --- |
| [RepositoryViewerController.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/RepositoryViewerController.scala) | `42d23d0321336101805430c911f7372ed12dd9de` | `ba353e4d9f7398886be79ab10df24c870cc44e76` |
| [JGitUtil.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/util/JGitUtil.scala) | `40d66603734958f97f5c6bd712b48fde505107d8` | `4990ab2e60915edcae345c86024c4f90a6e69e9c` |
| [PullRequestService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala) | `08348cedfcfe964b5bc289b8fd78b2c04496535c` | `3b335efcaaa393ff8e1dd79ace034462d0f54663` |
| [commit.scala.html](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/repo/commit.scala.html) | `bfc92a0f864eb0da3b7df89362cb121a3a734ecd` | `bfc92a0f864eb0da3b7df89362cb121a3a734ecd` |
| [editcomment.scala.html](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/repo/editcomment.scala.html) | `91e97f53daec8ef538045602c1b25ee9d637287d` | `91e97f53daec8ef538045602c1b25ee9d637287d` |

HTTP/MCP tests cover no-write preflight, old/new positions, exact forms and raw
text, definite rejection, reused IDs, malformed/redirect responses, failed
verification, races, deadlines and socket disconnects after saving. Live stdio
tests passed on disposable stock 4.44.0/4.48.0 images for owner and GUEST: three
positive posts, eight negative requests and a blocked read-only call per
version, with no ordinary conversation comment added. Existing read/history
checks and all 23 live E2Es also passed on each version. Reproduce through
[TESTING.md](../TESTING.md#review-comment-posts-p10).

Fork-origin PRs, other-PR shared-commit association, SSO/MFA-only sign-in,
deployment plugins and changed markup remain unverified as described in P08.
Reply/resolve/edit/delete and complete stored-history/raw-read APIs are not added.
