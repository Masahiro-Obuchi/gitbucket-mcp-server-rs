use serde::Deserialize;

use super::{
    client::GitBucketClient,
    outcome::Operation,
    profile::{ApiProfile, Route},
    url_safety,
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        operation::{OperationTarget, ResourceKind},
        pull_request::{canonical_sha, CreateForkPullRequest, PullRequest, PullRequestHead},
        repository::{Branch, Repository},
    },
};

#[derive(Debug, PartialEq, Eq)]
struct BranchTips {
    base: String,
    head: String,
}

#[derive(Deserialize)]
struct CommitIdentity {
    sha: String,
}

fn repository_matches(repository: &Repository, owner: &str, repo: &str) -> bool {
    repository.full_name == format!("{owner}/{repo}")
        && repository.name == repo
        && repository
            .owner
            .as_ref()
            .is_some_and(|user| user.login == owner)
}

fn observed_revision_matches(
    revision: Option<&PullRequestHead>,
    owner: &str,
    repo: &str,
    branch: &str,
    sha: &str,
) -> Option<bool> {
    let revision = revision?;
    let repository = revision.repo.as_ref()?;
    Some(
        revision.ref_name == branch
            && revision.sha.as_deref()? == sha
            && repository_matches(repository, owner, repo),
    )
}

async fn branch_tip(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<String> {
    let mut path = vec!["repos", owner, repo, "branches"];
    path.extend(branch.split('/'));
    let url = url_safety::append(client.base_url(), &path)?;
    let observed: Branch = client.get_url(url, "branch tip").await?;
    if observed.name != branch {
        return Err(GbMcpError::ReadConflict(
            "GitBucket returned a different branch",
        ));
    }
    observed
        .commit
        .map(|commit| commit.sha)
        .filter(|sha| canonical_sha(sha))
        .ok_or_else(|| GbMcpError::Other("GitBucket did not return a canonical branch tip".into()))
}

async fn branch_tips(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    change: &CreateForkPullRequest,
) -> Result<BranchTips> {
    let (base, head) = tokio::try_join!(
        branch_tip(client, owner, repo, &change.base),
        branch_tip(client, &change.head_owner, &change.head_repo, &change.head),
    )?;
    Ok(BranchTips { base, head })
}

async fn verify_branch_contains(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    branch: &str,
    tip: &str,
    commit: &str,
) -> Result<()> {
    if tip == commit {
        return Ok(());
    }
    // In standard GitBucket this endpoint returns branches that contain the
    // commit, with each branch's current tip.
    let branches: Vec<Branch> = client
        .get(&format!(
            "/repos/{owner}/{repo}/commits/{commit}/branches-where-head"
        ))
        .await?;
    let mut matching = branches
        .into_iter()
        .filter(|observed| observed.name == branch);
    let observed_tip = matching.next().and_then(|observed| observed.commit);
    if observed_tip.is_none_or(|observed| observed.sha != tip) || matching.next().is_some() {
        return Err(GbMcpError::ReadConflict(
            "The comparison base was not observed in the requested branch at its expected tip",
        ));
    }
    Ok(())
}

async fn fetched_head(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<String> {
    // GitBucket stores the form SHA in the PR record, but fetches the source
    // branch at creation time for this Git ref.
    let head_ref = format!("refs/pull/{number}/head");
    let [commit]: [CommitIdentity; 1] = client
        .get_with_query(
            &format!("/repos/{owner}/{repo}/commits"),
            &[("sha", &head_ref), ("per_page", "1")],
        )
        .await?;
    if !canonical_sha(&commit.sha) {
        return Err(GbMcpError::Other(
            "GitBucket did not return a canonical commit for the created PR head ref".into(),
        ));
    }
    Ok(commit.sha)
}

impl GitBucketClient {
    /// Create a PR from an explicit fork through GitBucket's verified Web form.
    pub async fn create_fork_pull_request(
        &self,
        owner: &str,
        repo: &str,
        change: &CreateForkPullRequest,
    ) -> Result<PullRequest> {
        url_safety::repository(owner, repo)?;
        url_safety::repository(&change.head_owner, &change.head_repo)?;
        change.validate()?;
        if owner == change.head_owner {
            return Err(GbMcpError::Unsupported(
                "The standard compare form for a fork requires different base and head owners"
                    .into(),
            ));
        }
        if self.api_profile() != ApiProfile::Standard {
            return Err(GbMcpError::Unsupported(
                "Fork pull request creation requires the standard profile's verified Web form"
                    .into(),
            ));
        }

        let session = self.web_session().await?;
        let (base_repo, head_repo) = tokio::try_join!(
            self.get_repository(owner, repo),
            self.get_repository(&change.head_owner, &change.head_repo),
        )?;
        if !repository_matches(&base_repo, owner, repo)
            || !repository_matches(&head_repo, &change.head_owner, &change.head_repo)
        {
            return Err(GbMcpError::ReadConflict(
                "GitBucket returned a different repository",
            ));
        }

        let before = branch_tips(self, owner, repo, change).await?;
        let form = session.fork_pull_request_form(owner, repo, change).await?;
        if form.head_sha != before.head {
            return Err(GbMcpError::ReadConflict(
                "The Web comparison head did not match the observed fork branch",
            ));
        }
        // The comparison base may precede both tips, but must be in both histories.
        tokio::try_join!(
            verify_branch_contains(
                self,
                owner,
                repo,
                &change.base,
                &before.base,
                &form.base_sha
            ),
            verify_branch_contains(
                self,
                &change.head_owner,
                &change.head_repo,
                &change.head,
                &before.head,
                &form.base_sha,
            ),
        )?;
        if branch_tips(self, owner, repo, change).await? != before {
            return Err(GbMcpError::ReadConflict(
                "A branch tip changed while preparing the fork pull request",
            ));
        }

        let mut target = OperationTarget::new(ResourceKind::PullRequest, owner, repo);
        target.commit = Some(before.head);
        let operation = Operation::new(
            "create_fork_pull_request",
            target,
            Route::Web,
            &[
                ("title", true),
                ("body", true),
                ("draft", true),
                ("author", true),
                ("state", true),
                ("merged", true),
                ("head", true),
                ("base", true),
                ("head_ref", true),
            ],
        );
        operation
            .run(async {
                let number = session
                    .post_fork_pull_request(owner, repo, change, &form)
                    .await?;
                operation.identity(number)?;
                let pr = self.get_pull_request(owner, repo, number).await?;
                operation.identity(pr.number)?;
                let checks = [
                    ("title", Some(pr.title == change.title)),
                    (
                        "body",
                        pr.body
                            .as_deref()
                            .map(|body| body == change.body.as_deref().unwrap_or_default()),
                    ),
                    (
                        "draft",
                        pr.draft.map(|draft| draft == change.draft.unwrap_or(false)),
                    ),
                    (
                        "author",
                        pr.user.as_ref().map(|user| user.login == session.login()),
                    ),
                    ("state", Some(pr.state == "open")),
                    ("merged", pr.merged.map(|merged| !merged)),
                    (
                        "head",
                        observed_revision_matches(
                            pr.head.as_ref(),
                            &change.head_owner,
                            &change.head_repo,
                            &change.head,
                            &form.head_sha,
                        ),
                    ),
                    (
                        "base",
                        observed_revision_matches(
                            pr.base.as_ref(),
                            owner,
                            repo,
                            &change.base,
                            &form.base_sha,
                        ),
                    ),
                ];
                let observed: Vec<_> = checks
                    .into_iter()
                    .filter_map(|(field, matched)| matched.map(|matched| (field, matched)))
                    .collect();
                operation.verify(&observed)?;
                if observed.len() != checks.len() {
                    return Err(GbMcpError::Other(
                        "GitBucket did not expose all fields needed to verify the created fork PR"
                            .into(),
                    ));
                }
                let head_sha = fetched_head(self, owner, repo, number).await?;
                operation.verify(&[("head_ref", head_sha == form.head_sha)])?;
                Ok(pr)
            })
            .await
    }
}
