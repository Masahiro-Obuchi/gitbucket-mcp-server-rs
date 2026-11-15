# Issue/PR milestone assignments (P15-4b)

This unit adds two tools, for 44 tools total: 21 reads and 23 writes.

| Tool | Required arguments after owner/repo | Result |
| --- | --- | --- |
| `set_issue_milestone` | `issue_number`, `milestone_number` | Observed complete Issue model |
| `clear_issue_milestone` | `issue_number` | Observed complete Issue model |

`issue_number` can identify an Issue or a PR. Both numbers must be in
1..2147483647. Setting replaces the current assignment with an existing
milestone from the same repository. Open and closed milestones are supported.
Clearing removes only the assignment and retains the milestone definition.
There is no zero, null or omitted-number clearing alias; use the clear tool.
Unknown arguments are rejected. Owner/repository identifiers are trimmed by MCP;
content and names are not rewritten.

## Route and permissions

These operations require `GITBUCKET_API_PROFILE=standard`. The default
`rest-only` rejects them before any HTTP, even with Web credentials. The verified
versions have no Issue REST PATCH; PR PATCH only applies title/body/state/base.
Sending milestone to those endpoints cannot establish a successful assignment.

The service first reads the scoped Issue and, for set, its scoped milestone,
checking both numbers. Missing/foreign references fail before sign-in or a write.
The baseline body and distinct label/assignee sets must be observable; missing
collections are not treated as empty. An already matching set is a read-only
no-op, including for a readable caller without developer permission, and needs
no Web credentials. It still requires the standard profile and valid reference.

Actual changes require matching PAT/Web credentials and the native developer
permission. PAT login, configured username and cookie-only login must match;
the cookie identity is checked immediately before writing. The Web request
contains only `milestoneId`, with a numeric string for set and an empty string
for clear, sent once to `/{owner}/{repo}/issues/{issue_number}/milestone` with the
AJAX header. PAT headers are not sent to Web. No redirects are followed and no
business request is replayed. Missing credentials or identity/session failures
before that attempt send no assignment write.

Read-only MCP mode hides and blocks both tools, including a matching no-op and
direct Rust handler calls. These are destructive, idempotent hints: they replace
or remove an assignment, and repeated matching calls do not write again. A real
change emits the native milestone timeline event and updates updated_at.

## Observations and failure behavior

After one Web POST, independently GET the same Issue and check its number.
Compare the requested milestone and preserve title, exact raw body, state,
label-name set and assignee-login set from the baseline. Order changes do not
change those sets; missing or duplicate collections cannot verify preservation.
Response HTML/progress and HTTP 200 alone do not establish success. No content,
state, label or assignee mutation is sent to repair an unexpected observation.

GitBucket omits an unassigned milestone from REST JSON. An omitted/null value
alone remains unknown. For clear, also GET the authenticated native Issue page
or PR conversation page and require exactly one `#label-milestone` marker with
the explicit native `No milestone` span. Missing, empty, duplicate, conflicting
or authentication/validation markup cannot prove clear. A native assigned link
is a mismatch. An already clear call uses the same positive marker, needs Web
credentials and returns without a business write. A readable GUEST can observe
that no-op; permission for actual changes remains native.

The controller updates storage **before** constructing its response; it can
return 404 after saving when a referenced milestone disappears or is invalid.
Therefore POST 404 is unknown, not definite rejection or proof that a route is
absent. Only POST 401/403 from its authenticator are definite rejection. Other
unexpected statuses, redirects, transport failures and invalid response pages
remain unknown. Any failed post-write REST or Web GET, including 401/403/404,
is uncertain; no retry or fallback follows it.

Operation errors retain the Issue/PR number, Web route and requested milestone
field plus preservation evidence. A different response number is recorded
separately. Mismatch, unknown and partial reflect observations: a matched
milestone alongside incomplete/changed preserved fields can be partial. A
deadline/cancellation keeps this progress. Re-read the resource before deciding
on another write. There is no rollback or atomic compare-and-swap guarantee;
sequential reference/baseline/post-write observations can race with other edits.
This unit does not verify preservation of every PR or milestone definition field.

## Primary sources and Rust interface

The relevant milestone route and empty marker contracts are present in both
official versions. Source files and Git blobs:

| Source | 4.44.0 | 4.48.0 |
| --- | --- | --- |
| [IssuesController.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/IssuesController.scala) | `a4b041800ee167061bcd1a614455c55630993206` | `2ffe371d21a1815f451f5f56b8196b5fcfe9a61d` |
| [IssuesService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/IssuesService.scala) | `983233381125247c0685627faa0e511bf163905b` | Same |
| [issueinfo.scala.html](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/issues/issueinfo.scala.html) | `bf19bca616e4a6dde0e7e05a43f375d5bb94abec` | `f240eea527bb2de45db37ebec85d8f665cf10bdf` |

[ApiIssueControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueControllerBase.scala)
has no update implementation;
[ApiPullRequestControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
passes only title/body/state/base to its update service. Assignment creation is
the separate [P15-4a contract](creation-metadata.md). Assignee updates and draft
transitions remain future units; they are not silently added to existing schemas.

GitBucketApi gains two required methods mirroring the tools and returning Issue.
Custom adapters must implement both; there are no default compatibility methods,
aliases, new runtime dependencies or configuration fields. GitBucketClient
provides inherent async methods with the same names. See
[TESTING](../TESTING.md#issuepr-milestone-assignments-p15-4b) for synthetic and
actual stdio coverage.
