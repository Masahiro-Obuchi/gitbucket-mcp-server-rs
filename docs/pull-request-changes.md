# Pull request commits, files and diff

P15 adds three read tools. The server now exposes 38 tools: 20 reads and 16
writes. All three work in read-only mode, reject unknown arguments, and require
a positive PR number and valid owner/repository identifiers.

| Tool | Source and authentication | Result |
| --- | --- | --- |
| `list_pull_request_commits` | One REST `/pulls/{number}/commits` request, PAT only | `commits`, `comparison`, paging metadata |
| `list_pull_request_files` | Explicit standard profile, matching PAT/Web identities, pinned Web patch | `files`, `comparison`, paging metadata |
| `get_pull_request_diff` | Same verified Web patch contract | Exact patch `chunks`, `comparison`, `diff` selection metadata and paging |

The verified GitBucket 4.44.0/4.48.0 commit endpoint uses the stored PR
`commitIdFrom..commitIdTo` range and returns a single array. It does not page;
the client sends no page/per_page and never replays it as a paged endpoint.
It preserves observed order, full commit message, Git author/committer/date,
parent SHAs/URLs and available linked users. Missing linked users remain null;
a Git author's name/email is not a GitBucket login. Invalid/repeated SHAs or a
nonempty revision range missing its observed head fail without a partial list.

The standard REST PR-files endpoint is unimplemented in both verified versions.
File/diff tools select the known Web route directly; they do not speculate on a
REST endpoint or switch routes after a REST error. Other profiles reject them
before HTTP. Web credentials are required even for these reads; authentication
uses the existing matching PAT/cookie identity checks and redirect boundaries.

## Comparison scope and races

Every initial read obtains the PR, validates its identity and explicit base/head
SHAs, fetches the source, then reads the PR again. Changed metadata produces
`read_conflict` and no successful partial result. `comparison` identifies the
source, `stored_base_sha`, `head_sha`, before/after metadata check and
`atomic: false`. Separate tool invocations can observe different revisions.

These are GitBucket's stored PR revision IDs. They are not the current base
branch tip and are still used when reading a closed/merged PR. The Web path
`/{owner}/{repo}/patch/{base}...{head}` compares those two trees through
JGit's getPatch; its three-dot URL spelling does not itself compute a new merge
base. Before/after equality cannot exclude an intervening change and reversal,
or provide a transaction over the source reads.

## File metadata and omitted content

Each file has exact nullable `old_path`/`new_path`, status, old/new Git mode,
`binary`, hunk count, additions/deletions and the upstream block byte length.
A null path represents the absent side of an addition/deletion. Hunk counts
exclude header lines and no-newline markers. Binary line counts are null,
not invented zeroes. Mode-only and empty-file blocks have no text hunks.
Modes retain symlink/gitlink distinctions; their textual patch represents link
targets or submodule commit IDs rather than the referenced file/tree contents.

Rename/copy status is returned only when explicitly present in the upstream
patch headers. The source may represent a rename as separate deletion/addition;
`comparison.rename_detection` states this limitation. The tool never guesses
renames by matching file content. Binary markers are preserved, but binary bytes
are unavailable (`binary_payloads_available: false`). Combined diffs, encoded
Git binary payloads, non-UTF-8 path escapes, ambiguous paths, unknown headers,
duplicate file blocks and incomplete hunks are unsupported, not empty success.
Headers over 16 KiB are also rejected to bound path parsing work.
Index IDs, absent sides and mode headers must agree. Changed object IDs require
text hunks or a binary marker, except for empty-file additions/deletions identified
by the empty-blob ID. Rename/mode headers alone do not excuse missing content.
Every patch line must end in LF, including a file's no-newline marker. These
structural checks cannot detect an upstream omission of entire complete blocks.

Git quoted/octal paths are decoded; whitespace and Unicode identities are kept.
The raw patch text remains unchanged, including quoted headers, CR in content
and no-newline markers. The server can only preserve the text GitBucket exposes;
it cannot restore original byte encodings already lost upstream.

## Paging and reconstruction

Commit/file lists use the existing `limit` (1..100, default 50), `cursor` and
`view` (summary default/full) contract. Summary commit-message previews and
nested collection limits are explicit in `_list`; full view retains the model
within limits. Paths and SHAs are never shortened into different identifiers.

Diff accepts `limit` and `cursor` but no view: its chunks always contain full
text. Each chunk is at most 4096 UTF-8 bytes and may end in the middle of a line
or hunk. `file_index` is the block's upstream index. `start_byte`/`end_byte`
are offsets in that file's raw patch block, and `file_patch_bytes` gives its
total. Concatenate chunks for each file in returned order, then files in index
order; follow `has_more`/`next_cursor` until false. A single chunk/page is not
necessarily a complete or applicable patch.

An optional exact `path` selects the one block matching old_path or new_path.
Missing/ambiguous matches fail within the observation, not with a claim that a
file was globally deleted. `diff` gives total/selected file and byte counts,
selected_path and chunk bound. The entire source is still fetched and validated
before selection, so a path filter cannot bypass the HTTP source limit.

All responses include both JSON and text fallback within the existing 64 KiB
CallToolResult bound. Byte limits can return fewer chunks/items than limit.
Observations are limited to 10,000 items/2 MiB serialized data; the 16-entry/
8 MiB cache expires after five minutes or earlier eviction/restart. Oversized
sources, items or observations fail without a successful prefix. Raising HTTP
limits does not raise these MCP limits. Full commit messages exceeding one
page's bound cannot be fetched in full through the commit-list tool.

Cursors bind tool, target, PR number, path selection, view, limit and issued
position. Continuations perform no HTTP and do not refresh PR state or account
permissions. Their comparison describes the original observation even after a
head update or permission change. Restart without cursor for a fresh read.

Example calls:

```json
{"name":"list_pull_request_commits","arguments":{"owner":"alice","repo":"project","pull_number":12,"view":"full","limit":10}}
{"name":"list_pull_request_files","arguments":{"owner":"alice","repo":"project","pull_number":12}}
{"name":"get_pull_request_diff","arguments":{"owner":"alice","repo":"project","pull_number":12,"path":"src/main.rs","limit":1}}
```

## Sources and verification

The upstream contracts are pinned by the official
[4.44 PR controller](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala),
[4.48 PR controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala),
[commit-list model](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiCommitListItem.scala)
(same blob `7a794745ce803ba6582b6ee0edbe25d55b2e851e` in both versions),
[PR revision model](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiPullRequest.scala),
[Web patch route](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/RepositoryViewerController.scala)
and [JGit patch formatter](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/util/JGitUtil.scala).

`tests/pull_request_changes_test.rs` and parser unit tests cover HTTP/MCP
behavior, exact text, file kinds, failures, source limits, cached continuations
and response bounds. The disposable `scripts/e2e/probe_changes.py` uses the actual
stdio binary to compare 28 commits and their full messages with Git, preserve
an observation across head advance, verify file metadata against Git numstat,
reconstruct the native Web patch, check path selection and read a merged PR.
Empty-file additions/deletions, symlink changes and a gitlink addition are included.
It is part of the core domain in [acceptance](acceptance.md).
