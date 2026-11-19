# Pull request creation from an explicit fork

`create_fork_pull_request(owner, repo, head_owner, head_repo, head, base, title, body?, draft?)`
creates a PR with destination `owner/repo` and source `head_owner/head_repo`.
The owners must differ. The source can be a renamed fork. Both branches are
exact Git branch names, including names with slashes. GitBucket must offer a
compare form for those repositories and branches. Use
[`create_pull_request`](../README.md#pull-requests) for the same repository or
GitBucket's same-name fork syntax.

The operation requires the standard profile, a PAT, matching Web credentials,
and repository and branch visibility through the PAT. GitBucket applies
the Web user's permissions to comparison and creation. The live fixture
uses a private fork whose owner has GUEST access on the destination.
Read-only mode hides and blocks the tool. The rest-only profile rejects the
operation before HTTP. See [the upstream contract](pull-request-review-contract.md).

The title is nonblank and single-line. Outer whitespace and control characters
are rejected in the title. A supplied body may contain internal LF, tabs and
Markdown spacing. The Web form trims titles and bodies, so input with outer
whitespace is rejected before HTTP. An omitted or empty body is observed as
an empty string. Omitted `draft` creates a ready PR.

## Verify the comparison and the created PR

1. Match the cookie-only Web session to the PAT. Read the exact repositories
   and both branch tips using the PAT.
2. Read the compare page from the named source. Require a unique native form,
   the exact destination form action and matching branch and repository inputs.
   Require its head SHA to equal the source branch tip. If the form's comparison
   base differs from the destination tip, check that the comparison base commit
   resolves in the destination repository. Recheck both tips and the Web
   identity before posting.
3. Submit the verified form once using the cookie. Take the PR number from a
   scoped 302 or 303 Location. Verify that number, author, title, body, open and
   unmerged state, draft, repositories, branches and comparison revisions with
   a separate PAT GET. Return the observed `PullRequest`.

The checks can detect changes before submission. A race window remains between
the checks and the POST, and the returned base SHA describes the comparison
rather than the destination's branch tip. Each call submits once and another
call can create a second PR. The redirect supplies the number without a
follow-up Web request.

If the POST or verification fails, inspect the operation's destination,
observed head SHA and any captured PR number before another call. An unexpected
redirect, a write 404 or a transport failure leaves the effect uncertain.
Successful observations and mismatches are recorded as field evidence.

Custom `GitBucketApi` implementations add the required
`create_fork_pull_request` method with `CreateForkPullRequest` input and
`PullRequest` output. See [the tests](../TESTING.md#creation-from-an-explicit-fork).
