# Issue/PR label assignments (P15-3)

These four tools manage assignments separately from repository label CRUD.
The server has 42 tools: 21 reads and 21 writes. All four use PAT-only REST under
both profiles. Only the list is registered in read-only mode; every write is
rejected before HTTP in that mode, even if it would be a no-op.

| Tool | Required input | Effect/result |
| --- | --- | --- |
| `list_issue_labels` | owner, repo, issue_number | Paged `labels` observation |
| `add_issue_labels` | owner, repo, issue_number, labels | Add missing names; preserve other assignments |
| `remove_issue_label` | owner, repo, issue_number, name | Remove one assignment; preserve other assignments |
| `replace_issue_labels` | owner, repo, issue_number, labels | Replace the complete set; [] clears it |

`issue_number` also accepts a PR number and must be 1..2147483647. The parent is
read and its number checked before label observation or mutation. Upstream label
endpoints can otherwise return empty sets or attempt changes for a missing
parent. A missing or inaccessible parent is an error before any write.

## Names, definitions and permission

Names are exact and compared case-sensitively by the client. They are not
trimmed. Inputs must be nonblank, at most 100 UTF-16 code units, without control
characters or dot segments; duplicate names are rejected rather than silently
deduplicated. Add requires at least one name; replacement accepts []. Removal
URI-encodes the exact name as one path segment. Read models retain observed
name, color, optional description and URL without inferring absent values.

**Add/replace create unknown repository labels.** GitBucket supplies a generated
color; the client does not invent a color or POST repository definitions itself.
Already existing colors/definitions are retained. Removing an assignment and
replacing/clearing the set do not delete repository label definitions. Inspect
definitions with `list_labels`/`get_label` and assignments with the new list.

GitBucket requires developer permission for actual assignment changes. Read
access is sufficient to observe a matching set without a write. An add whose
names are already assigned, replacement with the current set, or removal of an
absent assignment returns the checked existing set without sending a mutation.
Read-only MCP mode still rejects these write tools before checking their state.

## Requests, verification and uncertainty

Before a change, GET the parent and its unpaged label list. Compute the complete
expected set. Add sends one POST with only the requested names missing from that
set: upstream inserts each association directly, so posting an already assigned
name can cause a duplicate-key failure. Replace sends one PUT with the complete
requested names. Remove sends one DELETE to the exact name. DELETE's internal
label response and POST's new-label subset are not complete current observations.

After a write, GET the scoped parent and full assigned set again. Success
requires set equality, including all assignments expected to be preserved.
Duplicate names in an upstream observation are rejected. Success returns
`{issue_number, labels}` containing the complete observed label models. Write
results are not paged; their upstream responses retain the configured REST HTTP
bound. Use the list's bounded pages for large read observations.

The list shares [normal continuation limits](list-continuation.md): default
50/max 100 items per page, complete 64 KiB MCP response, 2 MiB/10000-item collected
observation, five-minute cache and earlier eviction/restart. Its cursor is bound
to this specific tool, repository, Issue/PR number, view and limit. Continuation
does not refetch revisions or permissions, including after a later label write.
The verified upstream endpoint is collected once without page/per_page.

Writes track target resource `issue_labels`, owner/repo and `number` for the
Issue/PR. The aggregate `labels` field covers the entire expected set, including
preservation checks; it does not assert individual batch-prefix outcomes.
Definite initial 4xx rejection is not_applied (408 remains uncertain). 5xx,
malformed/oversized responses, failed post-write reads and interruption can mean
unknown; a confirmed set mismatch is not_verified. Tool deadline/cancellation
keeps the same target/field evidence after an attempt. A batch can have created
repository definitions or a prefix of assignments before failing. No automatic
retry, fallback route, second write or rollback is sent. Re-read assignments
and definitions before deciding what to do after an uncertain outcome.

These are sequential observations, without compare-and-swap or an atomic set
snapshot. Concurrent changes can cause a mismatch even if requested names were
saved, or occur after verification. Replacement removes all assignments omitted
from its request, including ones introduced between its preflight and PUT.

## Primary sources

The official controller is identical in
[4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala)
and
[4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala),
Git blob `daa6fb0e3186d3bcf78ec2a9dfc76aa072c14b75`. It defines the four routes,
writable-user authorization, automatic definition creation, replacement deletion
and response shapes. The body is an object with a labels array as defined by
[AddLabelsToAnIssue.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/AddLabelsToAnIssue.scala),
blob `cd6f5eb4646ec39d2eb683d69836c2a585f84e40`.

[IssuesService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/IssuesService.scala),
blob `983233381125247c0685627faa0e511bf163905b`, scopes reads and writes by
repository and issue, inserts association/activity rows, and removes assignments.
[LabelsService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/LabelsService.scala),
blob `ac676d5ced03bed4233a8e0a1746bddf66736406`, generates the default color for
unknown definitions. Source reading is complemented by the HTTP/MCP regressions
and actual stdio contract probe described in
[TESTING](../TESTING.md#issuepr-label-assignments-p15-3).

`GitBucketApi` adds four required methods. Custom adapters must implement them;
`LabelNames` is the explicit request model and ResourceKind adds IssueLabels.
There are no compatibility aliases/defaults or new runtime dependencies/settings.
