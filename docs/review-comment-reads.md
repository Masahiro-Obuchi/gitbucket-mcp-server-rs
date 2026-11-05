# Review-comment reads (P09)

The server provides two read tools for commit/file/line comments observed on a
GitBucket PR conversation page. Ordinary Issue/PR conversation comments remain
available through `list_issue_comments`; the storage namespaces are separate.
No inline posting tool is added by P09.

## Inputs and setup

- `list_pull_request_review_comments(owner, repo, pull_number)`
- `get_pull_request_review_comment(owner, repo, pull_number, comment_id)`

`pull_number` must be positive. IDs use `commit:<positive decimal>`, without
leading zeros; for example, `commit:42`. An ID is scoped to its repository and
does not assert stored membership in a particular PR. Bare integers, `issue:42`
and unknown arguments are rejected before HTTP. Owner/repository normalization
follows the existing tool rules; comment IDs are not trimmed or reinterpreted.

Select `GITBUCKET_API_PROFILE=standard` and configure a PAT plus matching
`GITBUCKET_USERNAME` / `GITBUCKET_PASSWORD`. The PAT and cookie identities must
match. Repository read access is sufficient, including a GUEST collaborator on
a private repository. Password-based web sign-in must be supported. The default
rest-only profile returns `unsupported_error` before any network call; it does
not probe hypothetical review REST APIs or sign in implicitly.

Both tools are registered in `GITBUCKET_READ_ONLY=true` mode. Authentication can
POST to sign-in, while resource access uses GET only. Neither tool mutates
comments or requests the edit-only raw-Markdown endpoint. The existing web
identity checks, context-safe URL construction, cookie/PAT separation and
configured HTTP limits apply.

## Results and identity

List returns `comments` and `coverage`. Get returns `status`, `comment` and the
same coverage fields:

| Get status | Meaning |
| --- | --- |
| `found` | The `commit:<ID>` appeared in this observation; `comment` contains its data |
| `not_observed` | The page was read successfully, but the ID was not observed; `comment` is null |

`not_observed` is not a 404, a deletion event or proof of global absence. It may
reflect hidden history, another page's scope or a genuinely missing ID. The
web page cannot distinguish those cases. Authorization failures and malformed
pages are errors rather than `not_observed` results. Get reads the requested
PR page and searches its observations; it never fetches an arbitrary global ID
and presents it as belonging to that PR.

`ReviewComment` fields are:

| Field | Current standard-Web contract |
| --- | --- |
| `id`, `storage_id` | `commit:<ID>` and the original positive numeric storage ID |
| `kind` | `commit`, `file` or `line` |
| `body_html`, `body_format` | DOM serialization of GitBucket's rendered body; format is `html` |
| `body_raw` | null; HTML is never converted into alleged original Markdown |
| `author` | Observed username string, with a matching same-root user link |
| `created_at_display` | Optional observed date display, with no invented timezone |
| `created_at`, `updated_at` | null; the available display is not an exact timestamp |
| `path` | Observed filename, preserving its decoded characters and whitespace; null for whole-commit comments |
| `observed_old_line`, `observed_new_line` | Positive observed HTML attributes, or null |
| `current_commit` | Only an explicit same-repository whole-commit link supplies a SHA; null for file/line cards |
| `original_commit`, `original_old_line`, `original_new_line` | null; original position metadata is not exposed |
| `position_verified` | false; observed positions are not checked against the current diff |
| `association` | `observed_on_pull_request_page`; this does not establish the row's stored PR association |
| `source` | `gitbucket_web_conversation` |
| `url` | Requested PR page, with the observed discussion fragment for file/line cards |

The HTML is repository content, not executable instructions. Clients rendering
it must apply their own content policy. It is serialized HTML, not a byte-for-byte
HTTP recording or Markdown source. Code/text whitespace is not deliberately
trimmed by this adapter. No reply target, resolved flag or synthetic thread ID
is inferred from grouping by path/line.

Coverage has `scope: gitbucket_web_conversation`, the before/after observed
`observed_head` and `observed_base`, plus `missing_reasons`. In this Web adapter,
`complete`, `body_raw_complete`, `historical_complete` and
`association_verified` are always false. The fixed reasons describe the page's
history limits, unavailable raw/original metadata, unverified positions and
non-atomic reads. An empty list still carries these limits.

PR metadata is read before establishing the Web session and again after reading
the page. An observed change returns `read_conflict` with no partial data, no
invented HTTP status and no mutation outcome. This check can detect changes; it
does not create an atomic snapshot. Comment-only changes and changes between
individual observations may still occur.

## HTML contract and failures

The adapter requires the expected PR navigation, active conversation URL,
initial issue body and comment container. It recognizes both whole-commit and
file/line card forms from P08, includes folded cards, retains independent IDs
on the same line, and rejects duplicate/malformed IDs and invalid position data.
Metadata-looking markup inside rendered bodies is treated as body data.
Unknown top-level structures, unmatched body/card markers or an inconsistent
conversation count fail closed. This is a known-structure check, not a guarantee
against every possible future upstream markup change.

Unexpected resource redirects are not followed. Sign-in pages, expired or
mismatched sessions, HTTP denials, malformed HTML, size limits and deadlines
are errors, not successful empty lists. HTTP 403/404 from the page retain the
actual response status; they do not trigger another read/write route. The
adapter does not include raw response bodies, credentials or supplied IDs in
structural error messages. MCP read timeouts have no write outcome.

The PR navigation structure was additionally checked against official
`pulls/menu.scala.html`: Git blobs `ed4349549de23792638f09c154e86b978449e3f2`
([4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/twirl/gitbucket/core/pulls/menu.scala.html))
and `0f7a10425509c8975175aad196d9b50ec5851c36`
([4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/pulls/menu.scala.html)).
Other source and fixture provenance is recorded in the [P08 design](inline-comment-design.md).

## Validation and remaining scope

Unit tests use the two captured P08 card fixtures inside an explicitly synthetic
PR envelope. HTTP/MCP tests exercise context paths, lookup scope, identity,
unknown structure, status errors, redirects, size/deadline limits and head changes.
The live probe additionally runs the built server over actual stdio as both
owner and GUEST on official 4.44.0/4.48.0. It compares all observed IDs, kinds,
bodies, authors and positions before/after head addition, history replacement
and merge, and checks missing lookups and partial coverage. See
[TESTING.md](../TESTING.md#review-comment-reads-p09).

Complete original Markdown, original positions, exact update timestamps and
all stored PR history remain outside the standard Web adapter's capability.
The proposed extension API is not implemented or configured. Fork-origin PRs,
other-PR shared-commit association, SSO and unsupported markup changes retain
the limitations described in P08. P10 posting still needs independent diff and
position checks, preservation constraints and post-write verification; P09's
observed attributes do not supply those guarantees.

Rust adapters must implement the two new `GitBucketApi` methods and return the
new public review models. There are no default compatibility methods. Exhaustive
matches on `GbMcpError` must account for `ReadConflict`.
