# P08 inline-comment contracts

These fixtures come from `probe_inline_comments.py` against official, disposable
GitBucket 4.44.0 and 4.48.0 Docker environments. Unlike the model-shape fixtures in
`../gitbucket/`, these are observations of real HTTP responses. They are not full
HTTP recordings. No business repository or user data was used.

- `sources.json`: source URLs and exact Git blobs for both tags.
- `<version>/environment.json`: timestamp, image ID/digest, authentication and path.
- `<version>/observations.json`: HTTP methods/paths/statuses, submitted fixture
  fields, extracted metadata, saved textarea contents and snapshot observations.
- `<version>/conversation_before.html`: captured commit-comment card fragments,
  including the separate whole-commit DOM form.
- `<version>/new_line.html` and `special_path.html`: captured POST result cards.

The harness parses and reserializes selected DOM nodes, strips scripts, and
replaces the instance origin, generated users/repository and full commit SHAs.
The fixture origin is `http://gitbucket.test`, users are `probe-owner` and
`probe-reader`, and SHAs are BASE/FIRST/HEAD1/HEAD2/HEAD3. Short display SHA text,
comment IDs, timestamps, whitespace and body HTML remain as observed. These files
contain no headers, cookies, access tokens, passwords or complete auth pages.
Do not use them as browser-executable trusted HTML.
The scoped `.gitattributes` rule keeps intentional response whitespace out of
Git's trailing-whitespace diagnostics; fixture content is not trimmed.

The 11 commit-comment cases coexist with an IssueComment whose numeric ID is 1.
`conversation_before` and `files_before` contain all 11 commit comments, but the
normal comment is suppressed by the upstream same-ID grouping. The ordinary
Issue REST API still returns it. After head replacement, only IDs 5/6/7/8/10/11
remain on those pages; FIRST's page still shows commit IDs 1/9. These are coverage
limits, not a complete-history contract. An invalid path/line is deliberately
accepted upstream; a future adapter must validate it before posting.

Run offline checks with:

```bash
python3 -m unittest discover -s scripts/e2e -p 'test_*.py'
```

For live reproduction and the proposed P09/P10 constraints, see
[TESTING.md](../../../TESTING.md#inline-comment-contract-probe-p08) and
[the design](../../../docs/inline-comment-design.md).
