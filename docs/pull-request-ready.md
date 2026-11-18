# Mark a draft PR ready (P15-4d)

mark_pull_request_ready(owner, repo, pull_number) performs the verified native
draft-to-ready transition. It returns the complete observed PullRequest model.
The server exposes 50 tools: 21 reads and 29 writes.

This is one-way. The supported versions set isDraft to false; their REST PR
PATCH does not apply draft. There is no boolean/toggle argument, reverse
transition, implicit merge or substitute using a merge request. Create a new
draft with create_pull_request(draft=true). Ready-to-draft conversion is outside
the verified contract; plugins/future versions need a separate investigation.

## Input, profile and permission

MCP trims owner/repository identifiers and rejects unknown arguments. The PR
number must be 1..2147483647. Existing create/update schemas are unchanged;
passing draft to update_pull_request or this tool remains invalid.

The standard profile is required. Default rest-only returns unsupported before
all HTTP, even with Web credentials. Before any write, require the scoped PR's
number, open state, explicit merged=false and known draft boolean. Closed,
merged, unknown and incomplete snapshots are unsupported. Require observable raw
body, distinct label/assignee names, and complete canonical head/base repository,
branch and SHA-1 identities. Base repository identity must match the request.

An already ready PR is a read-only no-op and needs only the PAT. A readable
caller can observe that no-op without developer permission. Read-only MCP still
hides and rejects the write tool, including direct handlers and matching no-ops.
The tool advertises destructive=true and idempotent=true.

Actual changes require matching PAT/Web credentials. The native controller
requires base-repository read access and developer role on the **head**
repository, including a fork. Base ownership alone is not proof of permission.
A fork owner with base GUEST access can mark their PR ready; a base owner without
head developer role is refused. The server does not guess this permission from
reference visibility or an explicit-collaborator-only endpoint.

Native private-fork creation copies collaborators and grants the source owner
ADMIN on the fork. That default grant can authorize a base owner; it is not an
exception to the head-permission rule. The live probe checks default authorization
and then removes that disposable grant to prove the separate rejection case.
The fixture creates its fork through the common native Web form; 4.44.0 lacks
the REST fork-creation endpoint. This is test setup, not a product fork fallback.

## One write, bounded observations

Capture the initial head SHA in the operation target. After PAT/configured/
cookie identities match, re-read the PR and require the same content, state,
merged flag, head/base revisions and visible assignment metadata. A pre-write
change stops without POST. If another actor already made the otherwise unchanged
PR ready, return that observed no-op.

Immediately before one cookie-only AJAX POST to
/{owner}/{repo}/pull/{pull_number}/update_draft, recheck cookie identity.
The request body is empty. No PAT header is sent to Web and no resource redirect
is followed. The native response is only an acknowledgement, not proof of effect.

Independently GET the same PR afterward. Verify its number and draft=false, plus
baseline title, exact raw body, state, merged flag, head/base repository+ref+SHA
tuples, label-name set and visible assignee-login set. Order changes do not alter
sets; absent/duplicate data is unknown, not empty. Missing draft is unknown, not
false. A null mergeable value stays null and does not block readiness by itself.

Preservation is scoped to these observed fields. It does not establish that
every Issue metadata field (e.g. milestone), hidden suspended-user relation,
repository statistic, review or background status is unchanged. The live probe
also checks an assigned milestone independently, without extending the service's
verification claim.

These sequential reads are preflight checks, not atomic compare-and-swap. There
is no supplied expected_head_sha or externally reviewed-revision guarantee.
The head stored in operation.target.commit is the initially observed head.
Concurrent edits, close/merge or branch movement between checks can still occur
and are reported by the post-write observations where detectable.

## Errors and interruption

Native REST GET can return HTTP 200 with an empty body for a nonexistent PR.
The client cannot prepare a snapshot from it and returns an error before any
ready POST; it does not invent a 404 or a target resource from that response.

Native POST 401/403 and 404 are definite rejection. The native 404 covers the
pre-write missing-PR/head-developer guard; unlike the milestone assignment
controller, this handler has no post-save 404 lookup. Do not infer an absent API
or switch routes from it. Other unexpected statuses, redirects, transport
failures and HTTP 200 authentication/validation pages remain unknown.

Post-write GET failure, including 401/403/404, is uncertain and retains the
requested number and initial head. A different response number is recorded
separately. Operation fields distinguish requested draft and preserved title,
body, state, merged, head, base, labels and assignees. An ignored draft remains
not_verified. Verified readiness alongside missing/changed preserved fields can
be partial. Matching preserved fields alone does not prove readiness.

Deadline/cancellation after the attempt retains that progress. There is no
automatic retry, corrective mutation, fallback, reverse transition, merge,
rollback or search for another target. Re-read the identified PR before deciding
on another operation: readiness can survive an error.

## Primary sources and Rust interface

The relevant ready route and false-setting service contract are identical on
the two supported official versions.

| Source | 4.44.0 Git blob | 4.48.0 Git blob |
| --- | --- | --- |
| [PullRequestsController.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/PullRequestsController.scala) | f745adcc646ac9d199da85b0ffc217309496bbb4 | 8ba2bf65f7034f16e39f40d680ef4055831cb805 |
| [PullRequestService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala) | 08348cedfcfe964b5bc289b8fd78b2c04496535c | 3b335efcaaa393ff8e1dd79ace034462d0f54663 |
| [mergeguide.scala.html](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/pulls/mergeguide.scala.html) | 5c889d67b9f87c4e2411146bbfadbd86859ed4aa | cf162e87b75ada57c2c69390d3d2bd78e9c62599 |

The UI posts this route for Ready for review;
[ApiPullRequestControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
passes only title/body/state/base into its REST update service.
[ApiPullRequest.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiPullRequest.scala)
exposes the stored draft flag and the two repository revisions.
[RepositoryCreationService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/RepositoryCreationService.scala)
defines the default private-fork grants;
[RepositoryService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/RepositoryService.scala)
includes owners, group members, admins and developer/admin collaborators in its
developer-role test.

GitBucketApi gains one required mark_pull_request_ready method, returning the
existing PullRequest type. Custom adapters implement it explicitly; there are no
new models, settings, dependencies, aliases or compatibility default methods.
GitBucketClient provides the corresponding inherent async method. See
[TESTING](../TESTING.md#pull-request-ready-transition-p15-4d) for HTTP/MCP and
actual stdio validation.
