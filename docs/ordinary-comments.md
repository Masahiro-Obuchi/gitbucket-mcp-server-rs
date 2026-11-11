# Ordinary conversation comments (P15-2)

Three tools expose existing GitBucket REST operations. The server has 38 tools:
20 reads and 18 writes. All three work with only a PAT under both `rest-only`
and `standard`. Read-only mode registers the getter and hides/rejects the writes.

| Tool | Required input | Result |
| --- | --- | --- |
| `get_issue_comment` | owner, repo, comment_id | Complete `Comment` |
| `update_issue_comment` | owner, repo, comment_id, body | Stored `Comment`, after verification |
| `delete_issue_comment` | owner, repo, comment_id | `{deleted: true, comment_id: ...}` after verification |

`comment_id` is a numeric repository-scoped **conversation comment** ID from
creation or `list_issue_comments`, in 1..2147483647 (upstream signed Int range).
It is not an Issue/PR number. These operations also apply to PR conversation
comments, which use the same IssueComment storage and REST routes. Inline review
comments have their own storage and `commit:<ID>` identifiers and tools. No
parent-number alias or review-location arguments are accepted. All schemas reject
unknown fields. Owner/repo are validated and trimmed; IDs are validated before
HTTP, including when the Rust client is called directly.

## Text and author

`Comment` retains exact `body`, `user`, `created_at`, `updated_at` and `html_url`.
Missing optional data stays null; no author, timestamp or
parent is inferred from a URL. Detail getters do not preview or truncate text.
Lists still default to explicit summary projections; request `view: full` when
the complete cached body is needed. Existing HTTP limits apply to detail reads.

The edit first GETs the exact ID as a readable conversation comment. The native
PATCH can also mutate system timeline entries that the GET endpoint hides;
failed preflight reads or mismatched IDs therefore stop before PATCH. This check
is sequential and cannot exclude a later change to the target or permissions.
Preflight failures retain their normal error kinds because no write was attempted.
The edit replaces the entire body. MCP rejects blank text and sends other text
unchanged, including leading spaces, indentation, Unicode, trailing Markdown
hard-break spaces and final newlines. It sends one JSON PATCH with only `body`,
then GETs that ID. Success requires the PATCH ID, GET ID and exact stored body to
match. The returned model comes from GET: in both verified upstream versions,
PATCH incorrectly builds `user` from the editing account even when editing
another author's comment. GET joins the stored author. Returning PATCH directly
would misattribute the comment.

GitBucket authorizes reads with repository visibility. Editing/deletion requires
an authenticated readable user who is the author or has developer role. The
server lets GitBucket decide this from the PAT; it does not infer authorization
from cached authors or switch identity. Unauthorized writes can return 401,
including a valid PAT lacking permission. 404 is an API result, not evidence of
an unsupported endpoint, and never triggers Web fallback.

## Delete and failure observations

The controller returns the deleted Int directly. Scalatra interprets it as an
HTTP status: a deletion of ID 1 emits `HTTP/1.1 001 001`, rather than JSON `1`.
Other IDs can yield informational, redirect, error or nonstandard statuses,
without a response body. Informational statuses can delay verification until
the configured HTTP request timeout expires (30 seconds by default). A saved delete can therefore look like a transport
failure, 401 or 404. The client never follows a write redirect.

The client first GETs the exact comment; a missing/inaccessible comment stops
before DELETE. It sends one DELETE. A 4xx different from the requested ID, except
408, is a definite rejection under the verified contract. HTTP 408 remains uncertain.
Success codes and statuses equal
to the ID require verification; protocol/transport failures and other uncertain
responses also allow read-only verification. A later comment GET must return
404 and a repository GET must return the same visible `full_name` before
`deleted: true` is returned. The DELETE body is not used as confirmation.
A still-visible comment or failed verification cannot succeed. Losing repository
visibility leaves the result unknown. There is no second DELETE or Web fallback.
A repeated delete encounters the missing preflight comment and returns API 404
without sending a write. The delete annotation describes its idempotent effect;
repeated calls can return different results.

Update and delete create structured operation progress with resource
`issue_comment`, repository and `number` containing the comment ID. The parent
number remains absent because the request does not supply one. `body` or
`existence` field evidence accompanies post-write errors, tool deadlines and
cancellation. Definite initial 4xx rejections are `not_applied`; possible writes whose
verification fails are `unknown` or `not_verified`. A post-edit GET 404 does not
turn a completed PATCH into a rejected write. No fallback, retry or rollback is
sent. Re-read and inspect the outcome before deciding on another write.

The follow-up reads are sequential observations, without conditional writes or
an atomic snapshot. Concurrent edits, deletions and permission changes remain
possible. A post-delete 404 proves that the ID was not fetchable at that point;
the additional repository read checks visibility but cannot exclude transient
permission changes or concurrent deletion. A verified absence does not prove
which actor deleted the comment. No tombstone or audit-log
guarantee is claimed.

## Sources and verification

The official controller is identical in
[4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala)
and
[4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala):
Git blob `3e8892e69535fc368bed73f94f8ec83f399e2434`. It defines GET/PATCH/DELETE
`/api/v3/repos/:owner/:repository/issues/comments/:id`, the role/author check,
PATCH response identity and the raw Int deletion return. The malformed HTTP
status and the saved effect were observed directly against a disposable official
4.48.0 server.
The [IssuesService GET filter](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/IssuesService.scala)
includes `comment`, `close_comment` and `reopen_comment`, while the PATCH path
uses the broader `getComment` lookup. The [comment service](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/HandleCommentService.scala)
only deletes `comment` actions through REST: close/reopen-with-comment entries
can be read and edited but their deletion is rejected by the native endpoint.

`GitBucketApi` adds three required methods; custom adapters must implement them.
`UpdateComment` is the explicit replacement-body model. Existing comment
creation/listing and review routes are unchanged. There are no compatibility
aliases/default implementations or new runtime dependencies/configuration.

See [TESTING](../TESTING.md#ordinary-comment-operations-p15-2) for HTTP/MCP
regressions and the owner/GUEST actual stdio contract probe on both versions.
