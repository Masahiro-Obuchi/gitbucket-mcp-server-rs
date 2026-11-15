# Creation metadata (P15-4a)

This creation unit extended two existing tools, keeping the then-current count
of 42 tools (21 reads / 21 writes). [P15-4b](issue-milestones.md) adds two separate
milestone assignment tools, bringing the current total to 44.

Creation operations use PAT-only REST in both profiles. This unit adds no runtime
dependencies, configuration values, fallback routes or follow-up mutations.

| Tool | Optional creation fields | Verification |
| --- | --- | --- |
| `create_issue` | Existing `labels`, account `assignees`, new `milestone` number | Check references, then GET the created Issue and compare supplied metadata |
| `create_pull_request` | New `draft` boolean | For explicit true/false, GET the created PR and compare draft |

## Input and scope

Milestone is an existing repository milestone's number, 1..2147483647; omission
leaves it unassigned by the native default. Zero is not a clearing alias during
creation. Open and closed milestones can be selected on the verified versions.
The same `Milestone` model already exists on Issue responses.

Assignee identifiers are trimmed by the MCP tool; the Rust client expects
canonical identifiers. Distinct names are required. Each account is fetched and
its login checked, without inventing aliases or deriving identities from email.
Labels are exact, distinct, nonblank names within the assignment name limits;
each existing repository definition is fetched and its name checked. Unknown
labels are rejected before Issue creation because the native creation service
would ignore them. `add_issue_labels` / `replace_issue_labels` have their own
explicitly documented automatic-definition-creation semantics.

The milestone is fetched in the requested repository and its number checked.
Reference validation is read-only and happens before the one POST. It cannot
guarantee that references or permissions stay unchanged until creation.

`draft: true` creates a draft PR and `draft: false` a ready PR. Omission uses the
verified native default false. Explicit false is serialized, not omitted. The
existing PR get/list models and summary projection preserve draft, including
false; unknown data remains null. No default value is inferred into an absent
observation. Existing merge preflight rejects drafts.

PR creation does not expose assignees, milestone or maintainer_can_modify. The
verified controller does not apply those fields (even though its request type
mentions maintainer_can_modify). Issue/PR milestone updates use the separate
[set/clear contract](issue-milestones.md). Assignee and draft state updates
require a future operation contract and PR. Unknown fields are rejected
by MCP schemas and by the two public Rust creation request deserializers.

## Stored observations and failures

After a valid creation response, capture its nonzero number into the operation
target immediately. If Issue metadata was supplied, GET that Issue and compare
the supplied arrays as distinct name sets and milestone by number. Explicit []
is an empty request; a missing array is an unknown observation and cannot verify
it. Milestone null/missing is kept unknown when the requested assignment cannot
be observed. If draft was explicit, GET the PR and compare the observed boolean.
POST's metadata alone is not verification. GET must return the captured number;
an unexpected number is recorded separately in observed_number.

Only supplied metadata gets field evidence: labels, assignees, milestone or
draft. Title/body/branches are transmitted as before and are not upgraded to
verified fields by these checks. Body text is not trimmed. With no supplied
metadata, return the native creation response without an independent metadata
verification GET or a claim that defaults were verified. Success with supplied
metadata returns the complete observed GET model.

**GitBucket can create the Issue while ignoring metadata for a caller without
developer permission.** It does not reject the whole operation. Reference access
does not prove permission to assign, and the native collaborator-permission
endpoint does not cover every owner/group/admin case. The server therefore does
not infer authorization from reference visibility. Native permissions decide;
post-creation verification detects ignored assignments without another write.
Empty arrays can match a new Issue's empty state even for a readable caller.

A confirmed mismatch is not_verified, incomplete observations are unknown, and
verified requested fields alongside incomplete ones can produce partial. These
are observations, not proof that a creation was rolled back. A failed post-write
GET, including 401/403/404, remains an uncertain write with the captured number.
MCP deadline/cancellation retains progress after the attempt. Definite initial
creation 4xx rejection (except 408) is not_applied and invents no creation number.
If the POST response cannot be decoded, the number may remain unknown.

Inspect target.number and field evidence before another creation: a failed
verification can leave an Issue/PR. No automatic retry, corrective PATCH, Web
fallback, rollback/delete or search that invents a created ID is performed.
All observations are sequential and can race with concurrent changes; there is
no atomic metadata requirement or compare-and-swap guarantee.

## Primary sources and Rust API

Official 4.44.0 / 4.48.0 request sources are identical:

| Source at 4.48.0 (also checked at 4.44.0) | Git blob |
| --- | --- |
| [CreateAnIssue.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/CreateAnIssue.scala) | `16585b7fdb30a28639afb36b85670958a2621bc7` |
| [CreateAPullRequest.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/CreateAPullRequest.scala) | `112e721796ecf0a1491a4eaf18e7d85129029dd5` |
| [ApiIssueControllerBase.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueControllerBase.scala) | `599547dae90ceecbc2bb59c3078d4e6696a0c5d5` |

PR creation is defined in ApiPullRequestControllerBase.scala
([4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
blob `f2529bb2a8acb93be8e423866322ada649c2d148`,
[4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
blob `112460b0d7a96e006f218250f74a350ffcffc199`). Its create path passes draft and
does not apply assignment/milestone metadata.

[IssueCreationService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/IssueCreationService.scala)
blob `49fcd177ed99af980627db75af3d0934dd40584d` gates metadata on developer role
while permitting base Issue creation independently.
[MilestonesService.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/MilestonesService.scala)
blob `85d6788c7330f61f8ab818a742547cf4d4ce04e7` scopes milestone lookup by repository.
[Collaborator controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryCollaboratorControllerBase.scala)
blob `4a8ab09f8d5e5dfd7d8c7773056e518df7d4c215` looks up explicit collaborators for
its permission response.

The existing GitBucketApi methods take extended CreateIssue / CreatePullRequest
types; no new trait methods or compatibility defaults/aliases are added. Rust
struct literals must specify milestone/draft Option fields (None to omit them).
See [TESTING](../TESTING.md#creation-metadata-p15-4a) for HTTP/MCP regressions and
the actual stdio acceptance probe on the supported official versions.
