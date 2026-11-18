# Organization repositories and repository discovery (P15-6)

Both tools use PAT authentication through native REST in either API profile.

| Tool | Input | Result |
| --- | --- | --- |
| `create_organization_repository` | `organization`, `name`, optional `description`, `private`, `auto_init` | Verified repository details |
| `list_visible_repositories` | Optional `limit`, `view`, `cursor` | Page of repositories visible to the PAT |

## Create under an organization

The MCP tool trims organization and repository identifiers. Rust callers supply
canonical identifiers directly. Repository names follow GitBucket's character
rules and are limited to 100 characters. Both APIs preserve the description as
supplied. Omitted description, privacy and initialization use GitBucket's
defaults of empty description, public visibility and no initial branch.

First read `/users/{organization}` and require the exact login with type
`Organization`. GitBucket accounts share a namespace and the native creation
permission function allows the authenticated account name as a repository
owner. The explicit type check keeps this tool scoped to organizations.
GitBucket applies its creation policy to the authenticated user, including
organization membership and site administrator permissions.

Send one `POST /orgs/{organization}/repos`. Confirm the acknowledged owner and
name, then read the repository and its branches independently. The repository
read confirms the owner, name, description and privacy. Initialization expects
one default branch with a canonical commit SHA. An empty repository expects
no branches. The live fixture also checks the initialized README.

The tool returns the independently read repository. Post-write failures retain
the requested organization and repository and any verified fields. An error
after POST can leave a repository in place. Inspect that locator before calling
the tool again. The sequential reads can also detect concurrent changes to
metadata or branches. No request switches API route after a failure.
Read-only mode blocks creation before HTTP.

## Discover repositories across owners

GitBucket's source scopes `/user/repos` to the authenticated user's own
repositories. The official 4.48.0 image returned 404 for this path in the live
probe. To include private repositories shared by collaborators as well as group
memberships, discovery reads the current PAT account and the active user and
organization directories (`/users` and `/organizations`). It queries
`/users/{login}/repos` for every user, including the PAT owner, and
`/orgs/{login}/repos` for each organization. The same PAT is used
throughout. Native visibility filters determine the contents of each list.

The scan uses each directory login as one URL path segment, including
administrator-provisioned logins such as `builder@example.test`. Unaddressable
or duplicate owner logins and foreign or duplicate repository identities fail
the scan. The resulting list includes public repositories, visible personal and
organization repositories, and private repositories shared through native
collaborator access. Results are sorted by `full_name`.

GitBucket returns unpaged owner collections. The scan is sequential and bounded
to 500 directory entries, 10,000 repositories and the configured JSON byte
limit for the aggregate repository model data. Existing HTTP and tool timeouts
apply. An upstream error or a limit failure fails the observation. On a larger
instance use the owner-scoped `list_repositories` tool.

MCP exposes the observation through [bounded list pages](list-continuation.md)
and applies the shared 2 MiB budget to the chosen view before paging.
The default view is summary with a limit of 50. Follow `next_cursor` with the
same effective view and limit. Cursors belong to this tool and reuse cached
results for up to five minutes, subject to cache eviction. Permissions and data
can change during a scan and after the first page. A complete source observation
means every returned directory owner was queried successfully. Start a fresh
scan to refresh the observation.

## Upstream contract and Rust adapters

The native routes and their filters are shared by GitBucket 4.44.0 and 4.48.0.
See the repository API controllers for
[4.44.0](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryControllerBase.scala)
and [4.48.0](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryControllerBase.scala).
The [repository service](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/RepositoryService.scala)
applies the owner and visibility filters. The
[user controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiUserControllerBase.scala)
and [organization controller](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiOrganizationControllerBase.scala)
expose the active directories, and
[ApiUser](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/ApiUser.scala)
identifies groups as organizations.

`GitBucketApi` adds two required methods. Organization creation takes the
existing `CreateRepository` model and returns `Repository`. Discovery returns
`Vec<Repository>` to Rust callers. The MCP layer applies the shared paging and
summary projection. See [TESTING](../TESTING.md#organization-repositories-and-discovery-p15-6).
