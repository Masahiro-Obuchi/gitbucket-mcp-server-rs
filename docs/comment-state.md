# Comment with close/reopen (P15-5)

The server exposes 53 tools: 22 reads and 31 writes. Two explicit tools use the
native Issue comment endpoint for both Issues and unmerged PRs:

| Tool | Required input | Required starting state | Requested result |
| --- | --- | --- | --- |
| close_issue_with_comment | owner, repo, issue_number, body | open | closed plus one comment |
| reopen_issue_with_comment | owner, repo, issue_number, body | closed | open plus one comment |

Success returns CommentStateChange: `{comment, issue, pull_request}`. Comment and
Issue come from independent reads; pull_request contains the observed complete
PR model for a PR and is null for an Issue. PRs use their number as issue_number.
The response does not claim a transaction or an atomic snapshot.

## Input, profile and permission

Both rest-only and standard use PAT-only REST, without Web credentials. The MCP
layer trims owner/repo, rejects unknown arguments, and requires issue_number in
1..2147483647 and nonblank body. The Rust client validates the same inputs.
Nonblank content is sent and checked exactly, including leading/trailing spaces,
Unicode, indentation, Markdown hard breaks and final newlines. No dummy body is
generated. There is no action/state alias, optional empty body or PR-number alias.
Existing update and ordinary comment schemas are unchanged.

Require the exact readable parent, the opposite starting state and observable
raw body, distinct label names and distinct canonical visible assignee logins.
A matching requested state is rejected before POST, including repeats. It does
not append an ordinary comment while presenting it as a state change. For PRs,
also require matching Issue/PR numbers and state, explicit merged=false, known
draft and complete canonical head/base repository+ref+SHA-1 identities. The base
repository must match the request. Merged/unknown/incomplete PRs stop before POST.

Native authorization is base-repository developer role **or the Issue/PR opener**.
A GUEST opener can change their own Issue or PR. A readable non-opener without
developer role can still create a comment: the controller silently drops action
and saves an ordinary comment. The server does not infer permission from a
collaborator-only endpoint. Independent observations report a saved comment with
an ignored action as partial. This permission differs from PR readiness, whose
native guard checks head-repository developer role.

Both tools advertise destructive=true and idempotent=false. State alone can be
idempotent, but comments and notifications are not. Read-only MCP hides and
rejects both tools, including direct handlers and otherwise matching states.

## One native write and independent observations

Read the baseline and PAT actor, then send exactly one JSON POST:

```
POST /api/v3/repos/{owner}/{repo}/issues/{issue_number}/comments?action=close
{"body":"the exact supplied content"}
```

Reopen uses action=reopen. The operation does not split into comment plus PATCH
or use the existing Web state-only route. Web cookies are never sent. Native
handleComment changes state and creates a close_comment/reopen_comment entry
when authorized; existing state-only tools remain separate.

Capture the POST comment ID immediately. The POST acknowledgement/body/author
are not verification. Independently GET the parent Issue, the PR when relevant,
and the native **unpaged parent comment collection**. That collection must contain
exactly one entry with the created ID, exact body and PAT author. A standalone
comment getter lacks parent identity; a link is not used to infer membership.
The collection stays within the existing JSON byte limit; large or failed reads
leave the comment observation unknown and do not trigger a search/retry.

Verify requested state and retain Issue title, exact body, label-name set,
visible assignee-login set and Issue/PR classification. If the baseline has an
assigned milestone, verify its number. A baseline null/missing milestone does not
prove unassignment and adds no absence claim. PR observations also verify state,
merged, draft and head/base repository+ref+SHA identities. Missing/duplicate data
is unknown, not false or empty. The service does not claim complete preservation
of hidden assignee relations, every metadata field, timestamps, counts, hooks,
reviews or repository statistics. The live probe additionally checks base Git
tip stability. Native close/reopen does not perform a merge.

These reads are sequential. Another actor can edit/close/reopen/merge, move a
branch or change permissions between observations. An initial state check does
not prevent a concurrent state change before POST, and matching final state does
not prove which actor changed it. No atomic CAS or reviewed-head guarantee is
advertised. PR operation.target.commit is the initially observed head only.

## Errors, partial effects and interruption

Operation target.resource is issue_comment, parent_number is the requested
Issue/PR number and number is the created comment ID once known. Invalid POST
IDs and unexpected parent numbers remain errors. A foreign parent observation
records observed_parent_number while retaining both the intended parent and
created comment; it does not clear independent verified comment/PR observations.
Field evidence names comment.body, comment.author, comment.parent, state,
retained Issue fields and, for PRs, pull_request.* fields.

Initial read failures are ordinary preflight errors without write evidence.
POST 401/403 and other definite REST rejections are not_applied. **POST 404 is
unknown**: this controller can return NotFound after handleComment saves and a
final comment lookup fails. Its missing-parent path also returns 404, so status
alone cannot establish rejection. The local wrapper preserves the API status
as cause without changing other REST operations' status handling. Unexpected
redirects, transport/decoding failures, HTTP408 and server errors remain unknown;
there is no redirect follow or replay. Without a valid POST ID the service does
not guess the comment by matching text or list differences.

After a valid POST ID, independently observe as much as possible even if another
read fails or mismatches. Record each completed observation before the next
await. A verified comment with failed/unknown state, or verified state with an
unobserved/missing comment, is partial. Later GET401/403/404 does not turn the POST
into rejection. Deadlines/cancellation retain the captured ID, parent and verified
prefix. Inspect these locators and re-read before deciding on another write.
There is no automatic retry, compensating comment, corrective PATCH, fallback,
rollback, deletion or second POST.

Native close_comment/reopen_comment entries are returned by existing conversation
get/list and can be edited, but native REST only deletes plain comment entries.
The existing delete tool can therefore reject deletion of these action comments.
Editing/deleting a comment does not undo the parent state change.

## Primary sources and Rust interface

The relevant controller and service are identical on official 4.44.0 and 4.48.0:

| Source | Git blob on both tags |
| --- | --- |
| [ApiIssueCommentControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala) | 3e8892e69535fc368bed73f94f8ec83f399e2434 |
| [HandleCommentService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/HandleCommentService.scala) | 70a44c2d0a70c32309ae73493c9f6cf5bd3b36c4 |

The controller defines the action **query parameter**, required nonempty JSON
body, opener/developer filter and final lookup. The service defines close/reopen,
action-comment storage and native hooks. Existing [ordinary comment sources and
limits](ordinary-comments.md) apply to subsequent get/edit/delete.

GitBucketApi gains two required methods taking existing CreateComment and
returning new CommentStateChange in models::comment. Custom adapters implement
both explicitly; there are no compatibility default methods. OperationTarget
adds optional observed_parent_number; callers constructing it directly must
include that field, or use OperationTarget::new. No new runtime dependencies,
settings, aliases or defaults are introduced. See [TESTING](../TESTING.md#comment-with-closereopen-p15-5)
for HTTP/MCP and actual stdio coverage.
