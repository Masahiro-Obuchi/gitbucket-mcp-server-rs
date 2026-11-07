# Bounded MCP lists and continuations (P11)

All eight MCP list tools return pages, retaining the existing array keys:
repositories, branches, issues, issue comments, labels, milestones, pull requests
and PR review comments. There are still 32 tools, including 16 reads. Direct
`GitBucketClient` / `GitBucketApi` lists retain their Vec return types and
existing upstream traversal contracts.

## Input and migration

Every list accepts three optional arguments in addition to its existing target
and filters. Unknown fields remain invalid parameters.

| Argument | Contract |
| --- | --- |
| `limit` | Integer 1..100, default 50; an upper bound rather than a promised count |
| `cursor` | Omit for a fresh observation; otherwise reuse the exact server-issued string |
| `view` | `summary` (default) or `full` |

This changes the default response behavior. One call no longer means all items
or full models. Follow `has_more` / `next_cursor` until the observation is
exhausted. Increasing the limit to 100 is not complete enumeration. Repeat the
same normalized owner/repo, object number, state filter, effective view and
effective limit. Omitted state and explicit `open` are equivalent; omitted
view/limit mean `summary`/50 on every call. There is no caller-controlled sort;
upstream observed order is retained. Changing filters requires a fresh call.

Example first request to `list_issues`:

```json
{"owner":"team","repo":"project","state":"all","limit":50,"view":"summary"}
```

For the next request, repeat those arguments and add the returned `next_cursor`
verbatim. Never construct or edit cursors. They expire after five minutes and
are local to the server instance; restart/cache pressure can invalidate them
earlier. Unknown, malformed, expired, modified/unissued positions or mismatched
cursors return `validation_error` before HTTP, without a silent fresh search.

## Output and consistency

Each response contains the original array key, `has_more`, `next_cursor` (null
on the last page) and `range`. Review lists retain `coverage` on every page.

| `range` field | Meaning |
| --- | --- |
| `start`, `end`, `returned` | Zero-based inclusive start / exclusive end, and their difference |
| `observed_total` | Items collected in this observation, not a global history count |
| `limit`, `view` | Effective request options |
| `order`, `consistency`, `atomic` | `upstream_observed`, `cached_observation`, false |
| `source_complete` | Complete traversal of the selected source; false for partial Web review observations |
| `observation_id` | Identifier shared across pages of this observation |
| `observed_at_unix_ms`, `expires_at_unix_ms` | Collection timestamp and nominal expiry in Unix milliseconds; actual expiry uses a monotonic clock |
| `response_byte_limit` | 65536 bytes for the serialized MCP CallToolResult |
| `limited_by` | `count` or `bytes` when more items remain; null on the last page |

`has_more: false` exhausts the cached observation only. Review
`coverage.complete` and `range.source_complete` remain false on the last page.
Normal REST completeness means successful endpoint traversal, not an atomic
repository snapshot or completeness of unrelated data/history.

First calls use the existing verified APIs: Issue/PR lists read 25-item upstream
pages through the empty terminal page; unpaged endpoints are read once. Failures,
duplicate paged identities and the 100-request guard remain errors without
partial success. MCP `limit` does not change upstream requests or promise a
smaller download. Existing HTTP/body limits and whole-call deadlines still apply.
No speculative pagination parameters are sent to unpaged APIs.

The collected projection is retained temporarily. Continuations perform no HTTP,
returning the same observed values/order even if the source changes. Replaying
a valid cursor returns the same page. Initial collection can span concurrent
edits, so this is not an atomic source snapshot. Permission changes and new edits
are not rechecked on cached continuations; omit cursor for a fresh observation.
Detail getters independently read current data and can differ from cached lists.

## Summary and full views

Summaries select the fields below. `_list.omitted_fields` lists removed model
fields by JSON pointer, including removed null fields. Retained unknown values
remain null rather than becoming false/empty.

| List | Summary |
| --- | --- |
| Repositories | name/full_name, description preview, html_url, private/fork, default_branch, owner login, open_issues_count |
| Branches | name and commit SHA |
| Labels | name/color, description preview, url |
| Milestones | number/title/state, due_on, html_url, open/closed counts; omit description/creator detail |
| Issues | number/title/state, html_url, created/updated/closed times, comment count, author login and labels; omit body and other nested detail |
| Pull requests | number/title/state, draft/merged/mergeable, html_url, created/updated times, author login, head/base label/ref/SHA and repo full_name; omit body/repeated repository detail |
| Issue comments | id, body preview, user login, created/updated times, html_url |
| Review comments | id/storage_id/kind, author/path, observed sides/lines/current commit, position_verified/association/source/url/display date; omit body and original metadata |

Display strings, including comment bodies/titles, are limited to 1024 UTF-8
bytes at a character boundary; nested arrays to 20 items. Each affected field
is named in `_list.truncated_fields` using JSON pointers. Previews are exact
prefixes without an invented ellipsis, not complete Markdown. Names, logins,
author, path, ref, SHA, default branch and URLs remain exact. Oversized identities
fail rather than returning a wrong target. Empty truncation lists do not undo
explicit omissions.

`view: "full"` retains every existing model field without summary pruning/text
truncation, within the same page/observation limits. It adds no unavailable
upstream information. Full review bodies remain HTML with null raw Markdown and
partial coverage. Prefer detail get tools for Issue/PR/milestone/repository
content and rendered review bodies. Branches and ordinary comments currently
lack standalone detail tools; full list view can return their supported complete
models if each item fits. Giant full items are explicit errors, not silently
shortened text.

## Bounds and errors

- A page has at most 100 items and 64 KiB of serialized CallToolResult, including
  structuredContent, rmcp's text fallback and JSON escaping. The outer JSON-RPC
  envelope is excluded. Non-final pages always make progress.
- An observation has at most 10000 items and 2 MiB of serialized projected items
  plus coverage. Count overflow is `unsupported_error`; byte overflow is
  `response_too_large`. No page/cursor is issued if even one item cannot fit a
  page. Full view may require switching to summary or a detail getter.
- The cache holds at most 16 observations and 8 MiB of their serialized data,
  evicting the oldest first. These are serialized-data bounds, not a total
  process-memory guarantee. Expired entries are purged lazily on cache access;
  stopping the server releases the cache. Reads do not extend TTL. Cache Debug
  does not print items/cursors.

There are no new environment settings, legacy all-list mode or aliases. Public
Rust list Params gain the three optional fields; update struct literals. Shared
input types are `ListOptions` / `ListView`. The unused unbounded
`tools::response::success_list` helper is removed. Adapter signatures and detail
models stay unchanged. See [tests](../TESTING.md#list-response-pages-p11) and
[migration notes](../RELEASE.md).
