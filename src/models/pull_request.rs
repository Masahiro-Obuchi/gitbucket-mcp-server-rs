use serde::{Deserialize, Serialize};

use super::issue::Label;
use super::repository::Repository;
use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestHead {
    pub label: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: Option<String>,
    pub repo: Option<Repository>,
    pub user: Option<User>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub user: Option<User>,
    pub draft: Option<bool>,
    pub labels: Option<Vec<Label>>,
    pub assignees: Option<Vec<User>>,
    pub assignee: Option<User>,
    pub merged_by: Option<User>,
    pub html_url: Option<String>,
    pub url: Option<String>,
    pub commits_url: Option<String>,
    pub review_comments_url: Option<String>,
    pub review_comment_url: Option<String>,
    pub comments_url: Option<String>,
    pub statuses_url: Option<String>,
    pub head: Option<PullRequestHead>,
    pub base: Option<PullRequestHead>,
    pub merged: Option<bool>,
    pub mergeable: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePullRequest {
    pub title: String,
    pub head: String,
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePullRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// GitBucket updates title and body together; this is not the caller's patch.
#[derive(Debug, Serialize)]
pub(crate) struct GitBucketPullRequestUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<&'a str>,
    #[serde(flatten)]
    pub content: Option<GitBucketPullRequestContent<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitBucketPullRequestContent<'a> {
    pub title: &'a str,
    // Explicit null preserves an absent body when a title must be sent.
    pub body: Option<&'a str>,
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}
impl MergeMethod {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Merge => "merge-commit",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum MergeHeadGuard {
    #[default]
    None,
    Preflight,
    Atomic,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergePullRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_head_sha: Option<String>,
    pub merge_method: MergeMethod,
    pub head_guard: MergeHeadGuard,
}
impl MergePullRequest {
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::error::GbMcpError;
        if self.head_guard == MergeHeadGuard::Atomic {
            return Err(GbMcpError::Unsupported("GitBucket's verified REST contract does not enforce atomic expected-head conditions; no HTTP request was sent".into()));
        }
        match (self.head_guard,self.expected_head_sha.as_deref()) {
            (MergeHeadGuard::None,None)=>{},
            (MergeHeadGuard::Preflight,Some(sha)) if canonical_sha(sha)=>{},
            _=>return Err(GbMcpError::InvalidInput("expected_head_sha requires explicit head_guard=preflight and a full lowercase SHA-1; preflight requires that SHA".into())),
        }
        if self.merge_method == MergeMethod::Rebase && self.commit_message.is_some() {
            return Err(GbMcpError::Unsupported("GitBucket rebase ignores commit_message; omit it instead of silently losing supplied content".into()));
        }
        Ok(())
    }
}
pub(crate) fn canonical_sha(sha: &str) -> bool {
    sha.len() == 40
        && sha
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[derive(Debug, Serialize)]
pub(crate) struct GitBucketMergeRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<&'a str>,
    pub merge_method: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeVerification {
    pub head_guard: MergeHeadGuard,
    pub atomic_head_guard: bool,
    pub expected_head_sha: Option<String>,
    pub observed_head_sha: String,
    pub base_sha_before: String,
    pub mergeable_before: Option<bool>,
    pub pr_state_verified: bool,
    pub base_ref_verified: bool,
    pub commit_parents_verified: bool,
    pub merged_head_verified: bool,
    pub commit_message_verified: bool,
    pub repository_policy_preflight_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeResult {
    pub sha: String,
    pub merged: bool,
    pub message: Option<String>,
    pub requested_merge_method: MergeMethod,
    pub verification: MergeVerification,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_pull_request() {
        let json = r#"{
            "number": 10,
            "title": "Add feature X",
            "body": "This PR adds feature X",
            "state": "open",
            "user": {"login": "developer"},
            "html_url": "https://gitbucket.example.com/owner/repo/pull/10",
            "head": {
                "label": "developer:feature-x",
                "ref": "feature-x",
                "sha": "abc123"
            },
            "base": {
                "label": "owner:main",
                "ref": "main",
                "sha": "def456"
            },
            "merged": false,
            "mergeable": true,
            "created_at": "2024-01-01T00:00:00Z"
        }"#;

        let pr: PullRequest = serde_json::from_str(json).unwrap();
        assert_eq!(pr.number, 10);
        assert_eq!(pr.title, "Add feature X");
        assert_eq!(pr.state, "open");
        assert_eq!(pr.head.as_ref().unwrap().ref_name, "feature-x");
        assert_eq!(pr.base.as_ref().unwrap().ref_name, "main");
        assert_eq!(pr.merged, Some(false));
    }

    #[test]
    fn test_serialize_create_pull_request() {
        let create = CreatePullRequest {
            title: "New feature".to_string(),
            head: "feature-branch".to_string(),
            base: "main".to_string(),
            body: Some("PR description".to_string()),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["title"], "New feature");
        assert_eq!(json["head"], "feature-branch");
        assert_eq!(json["base"], "main");
    }

    #[test]
    fn test_serialize_update_pull_request_skips_missing_fields() {
        let update = UpdatePullRequest {
            state: Some("closed".to_string()),
            title: None,
            body: Some(String::new()),
            base: None,
        };

        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["state"], "closed");
        assert_eq!(json["body"], "");
        assert!(json.get("title").is_none());
        assert!(json.get("base").is_none());
    }

    #[test]
    fn test_serialize_merge_pull_request_minimal() {
        let merge = MergePullRequest {
            commit_message: None,
            expected_head_sha: None,
            merge_method: Default::default(),
            head_guard: Default::default(),
        };

        let json = serde_json::to_value(&merge).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["merge_method"], "merge");
        assert_eq!(obj["head_guard"], "none");
    }

    #[test]
    fn test_deserialize_merge_result() {
        let json = include_str!("../../tests/fixtures/gitbucket/merge-result.json");

        let result: MergeResult = serde_json::from_str(json).unwrap();
        assert!(result.merged);
        assert_eq!(result.sha, "c".repeat(40));
    }
}
