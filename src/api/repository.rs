use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::error::{GbMcpError, Result};
use crate::models::pull_request::canonical_sha;
use crate::models::repository::{Branch, CreateRepository, Repository};

use super::{client::GitBucketClient, outcome::Operation, profile::Route};
use crate::models::operation::{OperationTarget, ResourceKind};

const MAX_DISCOVERY_OWNERS: usize = 500;
const MAX_DISCOVERY_REPOSITORIES: usize = 10_000;

#[derive(Deserialize)]
struct AccountLogin {
    login: String,
}

fn matches_repository(repository: &Repository, owner: &str, name: &str) -> bool {
    repository.name == name && repository.full_name == format!("{owner}/{name}")
}

pub(crate) fn validate_new_repository_name(name: &str) -> Result<()> {
    super::url_safety::identifier(name, "repo")?;
    if name.len() > 100 {
        return Err(GbMcpError::InvalidInput(
            "Repository name must contain at most 100 characters".into(),
        ));
    }
    Ok(())
}

impl GitBucketClient {
    /// Observe repositories visible to the PAT across the native account directories.
    pub async fn list_visible_repositories(&self) -> Result<Vec<Repository>> {
        let actor = self.get_authenticated_user().await?;
        let users: Vec<AccountLogin> = self.get("/users").await?;
        let organizations: Vec<AccountLogin> = self.get("/organizations").await?;
        if users.len().saturating_add(organizations.len()) > MAX_DISCOVERY_OWNERS {
            return Err(GbMcpError::Unsupported(format!(
                "Repository discovery exceeds {MAX_DISCOVERY_OWNERS} owners; list repositories for a specific owner"
            )));
        }
        if !users.iter().any(|user| user.login == actor.login) {
            return Err(GbMcpError::ReadConflict(
                "The authenticated account is absent from the GitBucket user directory",
            ));
        }
        let mut owners = BTreeMap::new();
        for (collection, accounts) in [("users", users), ("orgs", organizations)] {
            for account in accounts {
                let invalid_login = || {
                    GbMcpError::ReadConflict(
                        "GitBucket account directory returned an invalid owner path segment",
                    )
                };
                // GitBucket can provision logins outside the interactive identifier rules.
                if account.login.contains(['/', '\\', '%']) {
                    return Err(invalid_login());
                }
                let url = super::url_safety::append(
                    self.base_url(),
                    &[collection, &account.login, "repos"],
                )
                .map_err(|_| invalid_login())?;
                if owners.insert(account.login, url).is_some() {
                    return Err(GbMcpError::ReadConflict(
                        "GitBucket account directories contain duplicate owner identities",
                    ));
                }
            }
        }
        let mut observed = BTreeSet::new();
        let mut repos = Vec::new();
        let mut bytes: usize = 0;
        let byte_limit = self.http_settings().json_limit_bytes;
        for (owner, url) in owners {
            // The instance applies the PAT's visibility in each owner-scoped
            // collection, including the authenticated user's own collection.
            let path = url.path().to_owned();
            let batch: Vec<Repository> = self.get_url(url, &path).await?;
            if repos.len() + batch.len() > MAX_DISCOVERY_REPOSITORIES {
                return Err(GbMcpError::Unsupported(format!(
                    "Repository discovery exceeds {MAX_DISCOVERY_REPOSITORIES} repositories; list by owner"
                )));
            }
            for repo in batch {
                super::url_safety::identifier(&repo.name, "repo")?;
                if !matches_repository(&repo, &owner, &repo.name)
                    || repo.owner.as_ref().is_some_and(|user| user.login != owner)
                    || !observed.insert(repo.full_name.clone())
                {
                    return Err(GbMcpError::ReadConflict(
                        "GitBucket repository discovery returned a foreign or duplicate identity",
                    ));
                }
                bytes = bytes.saturating_add(serde_json::to_vec(&repo)?.len());
                if bytes > byte_limit {
                    return Err(GbMcpError::ResponseTooLarge {
                        kind: "Aggregate repository discovery (try listing by owner)",
                        limit: byte_limit,
                    });
                }
                repos.push(repo);
            }
        }
        repos.sort_unstable_by(|left, right| left.full_name.cmp(&right.full_name));
        Ok(repos)
    }

    /// Create a repository under an existing organization through its REST route.
    pub async fn create_organization_repository(
        &self,
        organization: &str,
        body: &CreateRepository,
    ) -> Result<Repository> {
        super::url_safety::identifier(organization, "owner")?;
        validate_new_repository_name(&body.name)?;
        let account = self.get_user(organization).await?;
        if account.login != organization || account.user_type.as_deref() != Some("Organization") {
            return Err(GbMcpError::InvalidInput(
                "organization must name an existing GitBucket organization exactly".into(),
            ));
        }
        self.create_verified_repository(
            "create_organization_repository",
            &format!("/orgs/{organization}/repos"),
            organization,
            "Organization",
            body,
        )
        .await
    }

    async fn create_verified_repository(
        &self,
        operation_name: &'static str,
        create_path: &str,
        owner: &str,
        owner_type: &str,
        body: &CreateRepository,
    ) -> Result<Repository> {
        // GitBucket can authenticate provisioned logins such as user@example.test.
        // The login must still be safe to address as a single path segment.
        let invalid_owner =
            || GbMcpError::ReadConflict("GitBucket returned an unaddressable repository owner");
        if owner.contains(['/', '\\', '%']) {
            return Err(invalid_owner());
        }
        let repo_url = super::url_safety::append(self.base_url(), &["repos", owner, &body.name])
            .map_err(|_| invalid_owner())?;
        let branches_url = super::url_safety::append(repo_url.as_str(), &["branches"])
            .map_err(|_| invalid_owner())?;
        let repo_path = repo_url.path().to_owned();
        let branches_path = branches_url.path().to_owned();

        let operation = Operation::new(
            operation_name,
            OperationTarget::new(ResourceKind::Repository, owner, &body.name),
            Route::Rest,
            &[
                ("identity", true),
                ("description", true),
                ("private", true),
                ("auto_init", true),
            ],
        );
        let future = async {
            let created: Repository = self.post(create_path, body).await?;
            let identity_matches = |repo: &Repository| {
                matches_repository(repo, owner, &body.name)
                    && repo.owner.as_ref().is_some_and(|account| {
                        account.login == owner && account.user_type.as_deref() == Some(owner_type)
                    })
            };
            if !identity_matches(&created) {
                return Err(GbMcpError::Other(
                    "GitBucket returned a different or incomplete repository identity; inspect the requested repository".into(),
                ));
            }
            let repo = self.get_url(repo_url, &repo_path).await?;
            if !identity_matches(&repo) {
                return Err(GbMcpError::Other(
                    "The created repository identity could not be independently confirmed".into(),
                ));
            }
            let mut checks = vec![
                ("identity", true),
                (
                    "private",
                    repo.is_private == body.is_private.unwrap_or(false),
                ),
            ];
            if let Some(description) = &repo.description {
                checks.push((
                    "description",
                    description == body.description.as_deref().unwrap_or(""),
                ));
            }
            // Record the observed repository even if branch observation fails.
            let metadata = operation.verify(&checks);
            let branches: Vec<Branch> = self.get_url(branches_url, &branches_path).await?;
            let initialized = if body.auto_init.unwrap_or(false) {
                repo.default_branch
                    .as_ref()
                    .filter(|name| !name.is_empty())
                    .map(|default| {
                        branches.len() == 1
                            && branches[0].name == *default
                            && branches[0]
                                .commit
                                .as_ref()
                                .is_some_and(|commit| canonical_sha(&commit.sha))
                    })
            } else {
                Some(branches.is_empty())
            };
            if let Some(matches) = initialized {
                operation.verify(&[("auto_init", matches)])?;
            }
            metadata?;
            if initialized.is_none() || repo.description.is_none() {
                return Err(GbMcpError::Other(
                    "GitBucket did not expose all requested repository fields; inspect the repository".into(),
                ));
            }
            Ok(repo)
        };
        operation.run(future).await
    }

    /// List repositories for a user. Falls back to org endpoint on 404.
    pub async fn list_repositories(&self, owner: &str) -> Result<Vec<Repository>> {
        super::url_safety::identifier(owner, "owner")?;
        let result = self
            .get_with_query::<Vec<Repository>>(&format!("/users/{}/repos", owner), &[])
            .await;
        match result {
            Ok(repos) => Ok(repos),
            Err(crate::error::GbMcpError::Api { status: 404, .. }) => {
                self.get_with_query::<Vec<Repository>>(&format!("/orgs/{}/repos", owner), &[])
                    .await
            }
            Err(e) => Err(e),
        }
    }

    /// Get repository details.
    pub async fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        super::url_safety::repository(owner, repo)?;
        self.get(&format!("/repos/{}/{}", owner, repo)).await
    }

    /// Create a new repository for the authenticated user.
    pub async fn create_repository(&self, body: &CreateRepository) -> Result<Repository> {
        validate_new_repository_name(&body.name)?;
        let actor = self.get_authenticated_user().await?;
        if actor.user_type.as_deref() != Some("User") {
            return Err(GbMcpError::ReadConflict(
                "GitBucket did not identify the authenticated account as a user",
            ));
        }
        self.create_verified_repository(
            "create_repository",
            "/user/repos",
            &actor.login,
            "User",
            body,
        )
        .await
    }

    /// Fork a repository.
    pub async fn fork_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        super::url_safety::repository(owner, repo)?;
        let operation = Operation::new(
            "fork_repository",
            OperationTarget::new(ResourceKind::Repository, "", repo),
            Route::Rest,
            &[],
        );
        operation
            .run(self.post::<Repository, serde_json::Value>(
                &format!("/repos/{owner}/{repo}/forks"),
                &serde_json::json!({}),
            ))
            .await
    }

    /// List branches for a repository.
    pub async fn list_branches(&self, owner: &str, repo: &str) -> Result<Vec<Branch>> {
        super::url_safety::repository(owner, repo)?;
        self.get_with_query(&format!("/repos/{}/{}/branches", owner, repo), &[])
            .await
    }
}
