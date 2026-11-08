# Response contract fixtures

Issue and PR fixtures are synthetic values shaped from the official GitBucket
4.44.0/4.48.0 response models. `merge-result.json` is a synthetic normalized MCP
P12 success, including scoped verification; it is not the native merge response.
All contain no instance data or credentials and are not captured HTTP responses.
Native merge formats and source provenance are documented in the
[merge contract](../../../docs/pull-request-merges.md).

The Issue, PR and milestone source files are identical in the two tags.
Repository models differ as recorded below:

| Shape | Source | Git blob |
| --- | --- | --- |
| Issue | [ApiIssue.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiIssue.scala) | `eecb92fdaf2a3ee367d46c6a563654f102203e18` |
| Pull request | [ApiPullRequest.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiPullRequest.scala) | `64457b84359692c5c9525b1bb461b95104f0cd64` |
| Nested milestone | [ApiMilestone.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiMilestone.scala) | `77c9339137e670a63568c2b11b127ffbefc8ae93` |
| Repository 4.44.0 (missing fork flag) | [ApiRepository.scala](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/api/ApiRepository.scala) | `3f287281acfc234995b65489152c85f3c140a39c` |
| Repository 4.48.0 (explicit fork flag) | [ApiRepository.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiRepository.scala) | `5b623547405990410238c1ab95d4d4cab4758390` |

The PR fixture represents a merged PR with `draft=false` and `merged_by`.
A separate open draft response tests `draft=true`. `mergeable` is null because
the official implementation returns None. `closed_at` is absent from the Issue
response model; it must remain unknown
even if `updated_at` contains a later edit. Empty metadata arrays, omitted
metadata, explicit false booleans, and supplied closure dates are tested
separately. Comment/review URLs are retained as supplied metadata; their presence
does not claim that an inline review API is implemented.
Official 4.44.0 repository responses omit `fork`, while 4.48.0 supplies it.
The E2E creation test compares with raw REST metadata, requiring null for the
missing flag and retaining false when explicitly supplied.
