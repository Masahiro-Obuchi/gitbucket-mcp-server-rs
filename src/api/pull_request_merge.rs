//! A single REST merge, explicit guard semantics and independent result observations.
use super::{client::GitBucketClient, outcome::Operation, profile::Route, url_safety};
use crate::{
    error::{GbMcpError, Result},
    models::{
        operation::{OperationTarget, ResourceKind},
        pull_request::{
            canonical_sha, GitBucketMergeRequest, MergeMethod, MergePullRequest, MergeResult,
            MergeVerification, PullRequest,
        },
        repository::Branch,
    },
};
use serde::Deserialize;

#[derive(Deserialize)]
struct MergeResponse {
    sha: Option<String>,
    merged: Option<bool>,
    message: Option<String>,
}
#[derive(Deserialize)]
struct Commit {
    sha: String,
    parents: Vec<Parent>,
}
#[derive(Deserialize)]
struct Parent {
    sha: String,
}

fn returned_sha(value: &str) -> Result<String> {
    if canonical_sha(value) {
        return Ok(value.into());
    }
    // Stock 4.44 serializes ObjectId.toString; 4.48 serializes the containing
    // MergeResult.toString. Accept exact native frames, never a SHA substring.
    if let Some(sha) = value
        .strip_prefix("AnyObjectId[")
        .and_then(|v| v.strip_suffix(']'))
    {
        if canonical_sha(sha) {
            return Ok(sha.into());
        }
    }
    let parts = value
        .strip_prefix("MergeResult(AnyObjectId[")
        .and_then(|v| v.strip_suffix("))"))
        .and_then(|v| v.split_once("],List("));
    if let Some((sha, ids)) = parts {
        if canonical_sha(sha) && (ids.is_empty() || ids.split(", ").all(canonical_sha)) {
            return Ok(sha.into());
        }
    }
    Err(GbMcpError::Other("Merge response did not identify a canonical result commit; inspect the PR/base branch before retrying".into()))
}

fn revision(pr: &PullRequest, number: u64) -> Result<(&str, &str, &str)> {
    if pr.number != number {
        return Err(GbMcpError::Other(
            "GitBucket returned a different PR; no merge target can be established".into(),
        ));
    }
    let head = pr.head.as_ref().ok_or_else(|| {
        GbMcpError::Unsupported("Merge verification needs explicit head/base metadata".into())
    })?;
    let base = pr.base.as_ref().ok_or_else(|| {
        GbMcpError::Unsupported("Merge verification needs explicit head/base metadata".into())
    })?;
    match (head.sha.as_deref(), base.sha.as_deref()) {
        (Some(head), Some(base_sha)) if canonical_sha(head) && canonical_sha(base_sha) => {
            Ok((head, base_sha, &base.ref_name))
        }
        _ => Err(GbMcpError::Unsupported(
            "Merge verification needs canonical full head/base SHA-1 IDs".into(),
        )),
    }
}
async fn branch(client: &GitBucketClient, owner: &str, repo: &str, name: &str) -> Result<String> {
    let url = url_safety::append(client.base_url(), &["repos", owner, repo, "branches", name])?;
    let branch: Branch = client.get_url(url, "merge base branch").await?;
    if branch.name != name {
        return Err(GbMcpError::Other(
            "GitBucket returned a different base branch".into(),
        ));
    }
    branch
        .commit
        .map(|c| c.sha)
        .filter(|sha| canonical_sha(sha))
        .ok_or_else(|| GbMcpError::Other("Base branch omitted its canonical commit ID".into()))
}

pub(super) async fn merge(
    client: &GitBucketClient,
    owner: &str,
    repo: &str,
    number: u64,
    change: &MergePullRequest,
) -> Result<MergeResult> {
    change.validate()?;
    if number == 0 {
        return Err(GbMcpError::InvalidInput(
            "pull_number must be positive".into(),
        ));
    }
    let before = client.get_pull_request(owner, repo, number).await?;
    let (head, base_sha, base_branch) = revision(&before, number)?;
    if before.state != "open" || before.merged != Some(false) {
        return Err(GbMcpError::ReadConflict(
            "Merge requires an open, explicitly unmerged PR; no write was sent",
        ));
    }
    if before.draft != Some(false) {
        return Err(GbMcpError::Unsupported(
            "Draft or unknown draft state cannot be merged; no write was sent".into(),
        ));
    }
    if before.mergeable == Some(false) {
        return Err(GbMcpError::ReadConflict(
            "PR is explicitly unmergeable; no write was sent",
        ));
    }
    if change
        .expected_head_sha
        .as_deref()
        .is_some_and(|sha| sha != head)
    {
        return Err(GbMcpError::ReadConflict(
            "Expected head differs from the observed PR; no write was sent",
        ));
    }
    let base_tip = branch(client, owner, repo, base_branch).await?;
    let final_pr = client.get_pull_request(owner, repo, number).await?;
    if before != final_pr {
        return Err(GbMcpError::ReadConflict(
            "PR metadata changed during merge preparation; no write was sent",
        ));
    }
    let mut target = OperationTarget::new(ResourceKind::PullRequest, owner, repo)
        .numbered(number)
        .named(base_branch);
    target.commit = Some(head.into());
    let operation = Operation::new(
        "merge_pull_request",
        target,
        Route::Rest,
        &[
            ("merged", true),
            ("head", false),
            ("result_commit", true),
            ("commit_parents", true),
            ("base_ref", true),
        ],
    );
    let request = GitBucketMergeRequest {
        commit_message: change.commit_message.as_deref(),
        merge_method: change.merge_method.wire_name(),
    };
    let work = async {
        let response: MergeResponse = client
            .put_merge(
                &format!("/repos/{owner}/{repo}/pulls/{number}/merge"),
                &request,
            )
            .await?;
        let sha = returned_sha(response.sha.as_deref().unwrap_or(""))?;
        operation.result_commit(&sha);
        if response.merged != Some(true) {
            return Err(GbMcpError::Other(
                "Merge response did not report merged=true; inspect before retrying".into(),
            ));
        }
        let after = client.get_pull_request(owner, repo, number).await?;
        operation.identity(after.number)?;
        let (after_head, after_base_sha, after_base) = revision(&after, number)?;
        operation.verify(&[
            (
                "merged",
                after.merged == Some(true) && after.state == "closed",
            ),
            (
                "head",
                after_head == head && after_base == base_branch && after_base_sha == base_sha,
            ),
        ])?;
        let commit: Commit = client
            .get(&format!("/repos/{owner}/{repo}/commits/{sha}"))
            .await?;
        operation.verify(&[("result_commit", commit.sha == sha)])?;
        let parents: Vec<_> = commit.parents.iter().map(|p| p.sha.as_str()).collect();
        let parents_match = match change.merge_method {
            MergeMethod::Merge => parents == [base_tip.as_str(), head],
            MergeMethod::Squash => parents == [base_tip.as_str()],
            // Single-parent shape is observable; it is not a proof of complete replay equivalence.
            MergeMethod::Rebase => parents.len() == 1 && canonical_sha(parents[0]),
        };
        operation.verify(&[("commit_parents", parents_match)])?;
        let observed_tip = branch(client, owner, repo, base_branch).await?;
        operation.verify(&[("base_ref", observed_tip == sha)])?;
        Ok(MergeResult {
            sha,
            merged: true,
            message: response.message,
            requested_merge_method: change.merge_method,
            verification: MergeVerification {
                head_guard: change.head_guard,
                atomic_head_guard: false,
                expected_head_sha: change.expected_head_sha.clone(),
                observed_head_sha: head.into(),
                base_sha_before: base_tip,
                mergeable_before: before.mergeable,
                pr_state_verified: true,
                base_ref_verified: true,
                commit_parents_verified: true,
                merged_head_verified: change.merge_method == MergeMethod::Merge,
                commit_message_verified: false,
                repository_policy_preflight_verified: false,
            },
        })
    };
    operation.run(work).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_result_grammar_is_exact_and_does_not_extract_arbitrary_shas() {
        let sha = "b".repeat(40);
        let id = "a".repeat(40);
        for value in [
            sha.clone(),
            format!("AnyObjectId[{sha}]"),
            format!("MergeResult(AnyObjectId[{sha}],List())"),
            format!("MergeResult(AnyObjectId[{sha}],List({id}, {id}))"),
        ] {
            assert_eq!(returned_sha(&value).unwrap(), sha);
        }
        for value in [
            format!("prefix {sha}"),
            format!("MergeResult(AnyObjectId[{sha}],List(bad))"),
            format!("MergeResult(AnyObjectId[{sha}],List({id})))"),
        ] {
            assert!(returned_sha(&value).is_err());
        }
    }
}
