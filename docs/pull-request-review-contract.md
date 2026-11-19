# Renamed forks and pull request review state (P15-7)

This contract covers the official GitBucket 4.44.0 and 4.48.0 images with bundled
plugins, local authentication, and a root context path. The source tags resolve
to [`f16cc11` for 4.44.0](https://github.com/gitbucket/gitbucket/tree/f16cc117a9b0611c92a5bd799f5b5a3bfe08f842)
and [`4b2a781` for 4.48.0](https://github.com/gitbucket/gitbucket/tree/4b2a78174aa36bb47ece5391c6b7caf5c8432ce0).

| Capability | Standard GitBucket | MCP server |
| --- | --- | --- |
| Create a PR from a same-name fork | PAT REST accepts `head=owner:branch` | `create_pull_request` supports it |
| Create a PR from a renamed fork | The browser's compare form submits the head repository name separately | Creation requires the browser. `get_pull_request` can read the resulting PR |
| Comment on the conversation or on changed lines | Ordinary Issue comments and Web commit comments | Existing ordinary comment and inline review-comment tools |
| Approve, request changes, read a review decision, or resolve a review thread | No corresponding operation or stored state in either tested version | Unsupported |
| Report a build or validation result for a commit | A separate status is recorded by repository, SHA and context | Commit status is outside the review-comment tools |

## Fork identity and PR creation

Both REST controllers split `head` into owner and branch, then use the base
repository's name to look up the head repository under that owner. The
`CreateAPullRequest` input has no `head_repo` field. In the live fixture, the same
guest first created a PR from a same-name private fork with a PAT. After the
guest renamed the fork, its branch still resolved to the expected Git commit.
Both `owner:branch` requests, with and without an additional `head_repo`, returned
a JSON-wrapped 404 inside HTTP 200 on 4.44.0 and HTTP 404 on 4.48.0. The set of
open PR numbers stayed unchanged. The fixture leaves the old name vacant.
If that name is reused and the requested branch exists there, the REST lookup
can select the replacement repository as the PR head. Use the renamed fork's
Compare page to select the intended source repository explicitly.
[REST 4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
· [REST 4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
· [REST request type](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/CreateAPullRequest.scala)

The Web compare page is opened from the renamed fork. When both branches are
valid and there are commits to propose, the form targets the upstream
repository's `pulls/new` route. It carries the fork's exact owner, repository
and branch, the base branch, both comparison SHAs, and `isDraft`. The guest
submitted that generated form once. A subsequent REST GET and an MCP read
confirmed the new draft number, base and head repositories, refs and SHAs.
GitBucket trims the Web form's title and content. The fixture observed that
the stored body lost its leading and trailing whitespace. [Web 4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/PullRequestsController.scala)
· [Web 4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/PullRequestsController.scala)
· [Compare form](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/pulls/compare.scala.html)

A separate Web creation implementation would need an explicit source
repository, distinct from the REST `head` syntax. It would need to verify PAT
and Web identity, visibility and fork relationship, the generated form's
action and field bindings, and current source and destination revisions before
one POST. A comparison base SHA can be a merge base, so a future implementation
must distinguish it from the live destination branch tip. It would need to
reject input that the form would rewrite, and retain any PR identity from the
response for independent verification or uncertain outcomes. These checks
leave a race window before submission. An atomic revision guard would need
support from the server. [Comparison and creation service](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala)

## Review and thread state

The PR model stores its source, revisions, draft state and merge data. The
conversation and diff use Issue comments and commit comments. The latter have
file and line positions and optional PR association. They have no thread
resolution state. The `pull_request_review_comment` event and
`review_comments_url` are part of the webhook and response models. The
controllers do not register the corresponding GitHub-compatible comment and
review routes. The branch-protection implementation supports status checks,
with the review-protection sections left as comments. The Web control labeled
`Ready for review` changes the PR's [draft state](pull-request-ready.md).
[PR model](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/model/PullRequest.scala)
· [Comment models](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/model/Comment.scala)
· [Webhook event](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/model/WebHook.scala)
· [Branch protection API](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryBranchControllerBase.scala)

After marking the same PR ready, owner PAT requests to list reviews, requested
reviewers and PR review comments returned 404. Review submissions using
`APPROVE` and `REQUEST_CHANGES` returned 404 and left the observed ordinary
and inline comments intact. The same owner could create an ordinary comment
with REST, post and read an inline comment through stdio, and separately record
and read a successful commit status. The PR stayed open and ready with the
same revisions. The missing thread-resolution operation is
established by inspecting the standard Web routes and storage model. Commit
statuses describe checks for a SHA. An approval or a request for changes would
require a stored reviewer decision and its reviewed revision. A resolved
thread would require a stable thread identity and stored resolution state.
These would need an upstream or plugin contract with authorization and
readback semantics before MCP tools could expose them. [Commit status API](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryStatusControllerBase.scala)
· [Web commit comment controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/RepositoryViewerController.scala)

## Reproduce

The [disposable core fixture](../TESTING.md#renamed-forks-and-review-state-p15-7)
checks both versions. It uses the native Web fork and rename forms for setup
and reads the generated compare form before creating the draft. It writes only
synthetic repositories, users, commits and comments in the temporary instance.
