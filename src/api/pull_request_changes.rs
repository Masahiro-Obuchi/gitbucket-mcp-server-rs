//! One unpaged REST list or pinned Web patch, bracketed by PR metadata reads.
use super::{
    change_patch, client::GitBucketClient, profile::ApiProfile, review_comment, url_safety,
};
use crate::{
    error::{GbMcpError, Result},
    models::{pull_request::PullRequest, pull_request_changes::*},
};
use std::collections::HashSet;

fn target(owner: &str, repo: &str, number: u64) -> Result<()> {
    url_safety::repository(owner, repo)?;
    if number == 0 {
        return Err(GbMcpError::InvalidInput(
            "pull_number must be positive".into(),
        ));
    }
    Ok(())
}
fn comparison(pr: &PullRequest, number: u64, source: &str) -> Result<ChangeComparison> {
    let (head, base) = review_comment::revision(pr, number)?;
    Ok(ChangeComparison {
        source: source.into(),
        stored_base_sha: base,
        head_sha: head,
        atomic: false,
        pr_metadata_checked_before_after: true,
        rename_detection: if source == "rest_pr_commits" {
            "not_applicable"
        } else {
            "upstream_patch_only_may_split_delete_add"
        }
        .into(),
        binary_payloads_available: false,
    })
}
fn unchanged(before: &PullRequest, after: &PullRequest) -> Result<()> {
    if before != after {
        return Err(GbMcpError::ReadConflict("PR metadata changed while reading changes; no partial result was returned. Restart the observation."));
    }
    Ok(())
}
impl GitBucketClient {
    pub async fn list_pull_request_commits(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PullRequestCommits> {
        target(owner, repo, number)?;
        let before = self.get_pull_request(owner, repo, number).await?;
        let comparison = comparison(&before, number, "rest_pr_commits")?;
        // GitBucket ignores page/per_page on this endpoint: fetch exactly once.
        let commits: Vec<PullRequestCommit> = self
            .get(&format!("/repos/{owner}/{repo}/pulls/{number}/commits"))
            .await?;
        let mut seen = HashSet::new();
        for c in &commits {
            if !crate::models::pull_request::canonical_sha(&c.sha)
                || !seen.insert(c.sha.clone())
                || c.parents
                    .iter()
                    .any(|p| !crate::models::pull_request::canonical_sha(&p.sha))
            {
                return Err(GbMcpError::Other(
                    "PR commit response contains invalid or repeated commit identities".into(),
                ));
            }
        }
        if comparison.head_sha != comparison.stored_base_sha && !seen.contains(&comparison.head_sha)
        {
            return Err(GbMcpError::Other(
                "PR commit range did not include its observed head; no partial list was returned"
                    .into(),
            ));
        }
        unchanged(&before, &self.get_pull_request(owner, repo, number).await?)?;
        Ok(PullRequestCommits {
            commits,
            comparison,
        })
    }
    async fn read_change_patch(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<(String, ChangeComparison)> {
        target(owner, repo, number)?;
        if self.api_profile() != ApiProfile::Standard {
            return Err(GbMcpError::Unsupported("PR files/diff require the standard Web patch contract; no speculative REST or Web request was sent".into()));
        }
        if self.web_credentials().is_none() {
            return Err(GbMcpError::Authentication(
                "PR files/diff require matching PAT and Web credentials",
            ));
        }
        let before = self.get_pull_request(owner, repo, number).await?;
        let comparison = comparison(&before, number, "web_pinned_patch")?;
        let session = self.web_session().await?;
        let patch = session
            .review_patch(
                owner,
                repo,
                &comparison.stored_base_sha,
                &comparison.head_sha,
            )
            .await?;
        unchanged(&before, &self.get_pull_request(owner, repo, number).await?)?;
        Ok((patch, comparison))
    }
    pub async fn list_pull_request_files(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PullRequestFiles> {
        let (patch, comparison) = self.read_change_patch(owner, repo, number).await?;
        let files = change_patch::parse(&patch)?
            .into_iter()
            .map(|b| b.file)
            .collect();
        Ok(PullRequestFiles { files, comparison })
    }
    pub async fn get_pull_request_diff(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        path: Option<&str>,
    ) -> Result<PullRequestDiff> {
        if path.is_some_and(|p| p.is_empty() || p.contains('\0')) {
            return Err(GbMcpError::InvalidInput(
                "path must be a nonempty exact Git path without NUL".into(),
            ));
        }
        let (patch, comparison) = self.read_change_patch(owner, repo, number).await?;
        let blocks = change_patch::parse(&patch)?;
        let total_files = blocks.len();
        let selected: Vec<_> = blocks
            .into_iter()
            .enumerate()
            .filter(|(_, b)| {
                path.is_none_or(|p| {
                    b.file.old_path.as_deref() == Some(p) || b.file.new_path.as_deref() == Some(p)
                })
            })
            .collect();
        if path.is_some() && selected.len() != 1 {
            return Err(GbMcpError::InvalidInput("path was absent or ambiguous in the observed patch; use exact old_path/new_path from list_pull_request_files".into()));
        }
        let selected_files = selected.len();
        let selected_patch_bytes = selected.iter().map(|(_, b)| b.patch.len()).sum();
        let mut chunks = Vec::new();
        for (file_index, b) in selected {
            let mut start = 0;
            while start < b.patch.len() {
                let mut end = (start + 4096).min(b.patch.len());
                while !b.patch.is_char_boundary(end) {
                    end -= 1;
                }
                chunks.push(DiffChunk {
                    file_index,
                    old_path: b.file.old_path.clone(),
                    new_path: b.file.new_path.clone(),
                    start_byte: start,
                    end_byte: end,
                    file_patch_bytes: b.patch.len(),
                    text: b.patch[start..end].into(),
                });
                start = end;
            }
        }
        Ok(PullRequestDiff {
            chunks,
            comparison,
            selected_path: path.map(str::to_owned),
            total_patch_bytes: patch.len(),
            selected_patch_bytes,
            total_files,
            selected_files,
        })
    }
}
