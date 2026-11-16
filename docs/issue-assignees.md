# Issue/PR assignee updates (P15-4c)

Three tools change individual account assignments on either an Issue or a PR.
The server exposes 48 tools: 21 reads and 27 writes.

| Tool | Arguments after owner/repo/issue_number | Effect |
| --- | --- | --- |
| add_issue_assignees | assignees: nonempty array of logins | Add missing accounts, retaining all existing assignees |
| remove_issue_assignee | username: one login | Remove that account, retaining the others |
| replace_issue_assignees | assignees: array of logins | Replace the complete set; [] clears assignments |

All return the complete observed Issue model, including for PR targets. Existing
get_issue exposes assignments; there is no additional list tool or cursor type.
An Issue/PR number must be in 1..2147483647. MCP trims identifiers, then rejects
blank, invalid or duplicate logins, including duplicates introduced by trimming.
Rust clients pass canonical identifiers in AssigneeNames. Unknown arguments and
request-model JSON fields are rejected. Title/body/label text is not rewritten.

## Route, account scope and no-ops

The standard profile is required; default rest-only refuses before any HTTP.
The supported versions have no Issue REST PATCH and their PR PATCH does not
apply assignees. No guessed endpoint or ignored JSON field is sent.

Before writing, read the repository-scoped Issue and check its number. Require
observable body, distinct label names and distinct canonical individual
assignees. Every supplied account, including remove's username, is fetched and
must have the exact login and type User. Missing/suspended accounts, aliases,
Organization/group accounts and unknown types are unsupported; these tools do
not create accounts or grant repository membership. The native route permits
active individual GUEST and non-collaborator accounts as assignees, beyond the
developer-only suggestion list in the UI. Visibility of a reference does not
grant the caller permission to mutate assignments.

Every call, including a matching no-op, requires matching PAT/Web credentials.
PAT login, configured username and cookie-only login must match. Independently
read the native Issue/PR sidebar and reconcile its complete assigned set with
REST. A matching add/replace or absent remove returns without a business POST.
A readable GUEST can observe a matching no-op; actual changes require the native
developer permission. Read-only MCP hides and blocks all three tools, including
direct handler calls and no-ops.

REST excludes assignments to suspended/deleted accounts. The native sidebar can
retain such a row without a canonical profile link. This must not become [] or
a successful clear/no-op: all rows must be independently identifiable. Missing,
duplicate, malformed, foreign/context-mismatched links, non-individual accounts,
or disagreement between REST and Web stop the operation. The sidebar's explicit
No one assigned marker is required for empty; a missing container is unknown.
Display names and avatar links are not identities.

## Writes and observations

Compute missing and extra names from the proven baseline. Add missing names
first, then remove extras, each in canonical order. Each step sends one
cookie-only AJAX POST, containing only assigneeUserName, to the native
issues/{number}/assignee/new or issues/{number}/assignee/delete route. Cookie
identity is checked immediately before every POST. No PAT header reaches Web.
Duplicate additions are skipped because native insertion is not duplicate-safe.
No redirects or business-request retries are followed.

After each step, GET the same Issue and verify its number. Independently read
the sidebar again, requiring complete agreement on assignees. Compare the full
expected intermediate set, not only the affected account, and preserve baseline
title, exact raw body, state, labels and milestone. Missing/duplicate arrays stay
unknown. An absent REST milestone is verified with the native explicit
No milestone marker; omission alone is not proof of preservation. HTTP 200 or
an empty AJAX response alone does not prove storage.

Any unexpected state stops later writes. In particular, an unexpected concurrent
addition does not cause a planned duplicate POST, even if it happens to satisfy
the final set. These are sequential observations, not an atomic replacement or
compare-and-swap guarantee. Native timeline entries and addition hooks can be
emitted before insertion fails. The service verifies assignments, not the
absence or delivery of every timeline/plugin side effect.

## Failure evidence

Operation errors retain the Issue/PR number and Web route. The aggregate
assignees field describes the final target set; requested assignee.LOGIN fields
describe individual planned membership changes, including removals. The latter
use owned field names in the shared progress tracker so evidence survives
deadline/cancellation and later steps. A verified prefix remains visible as
partial if a later account fails; an unattempted account remains not_attempted.
An unexpected response number is recorded separately without replacing the
intended target. If this happens after a later step, only that step's fields
become unknown; earlier verified accounts and unattempted changes are retained.

Only native POST 401/403 are definite authenticator rejections. Other unexpected
statuses (including 404/422/5xx), redirects, malformed/validation/authentication
pages and transport failures remain unknown. Post-write REST/Web failures remain
uncertain even for 401/403/404. Known preservation observations survive later
verification-read failure. A matched requested account alongside an unknown or
changed preserved field can produce partial; matching preserved fields alone
do not prove that an assignment was applied.

A successful prefix is not undone. No retry, compensating mutation, route
fallback, account restoration or rollback is performed. Inspect the target and
per-account evidence, then re-read before deciding on another operation.

## Sources and interface

The assignee new/delete routes and sidebar contract were checked in the official
4.44.0 and 4.48.0 sources and exercised on both images.

| Source | 4.44.0 Git blob | 4.48.0 Git blob |
| --- | --- | --- |
| [IssuesController.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/IssuesController.scala) | a4b041800ee167061bcd1a614455c55630993206 | 2ffe371d21a1815f451f5f56b8196b5fcfe9a61d |
| [IssuesService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/IssuesService.scala) | 983233381125247c0685627faa0e511bf163905b | Same |
| [issueinfo.scala.html](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/issues/issueinfo.scala.html) | bf19bca616e4a6dde0e7e05a43f375d5bb94abec | f240eea527bb2de45db37ebec85d8f665cf10bdf |

[AccountService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/AccountService.scala)
filters removed accounts from user lookup; suspension retains IssueAssignees.
[helpers.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/view/helpers.scala)
omits profile links for unavailable accounts, rather than inventing their login
from display text.
[ApiIssueControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueControllerBase.scala)
filters assignments through available accounts in its GET projection.

GitBucketApi gains three required methods and the public AssigneeNames request
type. Custom adapters implement them explicitly; no compatibility defaults,
aliases, dependencies or settings are added. Existing Issue/PR creation and
update schemas are unchanged. Draft-to-ready uses [its separate tool](pull-request-ready.md); reverse
conversion remains unsupported.
See [TESTING](../TESTING.md#issuepr-assignee-updates-p15-4c) for HTTP/MCP,
parser and actual stdio coverage.
