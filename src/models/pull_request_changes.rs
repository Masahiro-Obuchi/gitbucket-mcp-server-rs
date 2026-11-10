//! Observable PR changes between stored base/head IDs, not a fresh base branch tip.
use super::user::User;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitIdentity {
    pub name: String,
    pub email: String,
    pub date: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestCommitMessage {
    pub message: String,
    pub author: CommitIdentity,
    pub committer: CommitIdentity,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitParent {
    pub sha: String,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestCommit {
    pub sha: String,
    pub commit: PullRequestCommitMessage,
    pub author: Option<User>,
    pub committer: Option<User>,
    pub parents: Vec<CommitParent>,
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeComparison {
    pub source: String,
    pub stored_base_sha: String,
    pub head_sha: String,
    pub atomic: bool,
    pub pr_metadata_checked_before_after: bool,
    pub rename_detection: String,
    pub binary_payloads_available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestCommits {
    pub commits: Vec<PullRequestCommit>,
    pub comparison: ChangeComparison,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequestFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileChangeStatus,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub binary: bool,
    pub hunks: usize,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub patch_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestFiles {
    pub files: Vec<PullRequestFile>,
    pub comparison: ChangeComparison,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffChunk {
    pub file_index: usize,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
    pub file_patch_bytes: usize,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestDiff {
    pub chunks: Vec<DiffChunk>,
    pub comparison: ChangeComparison,
    pub selected_path: Option<String>,
    pub total_patch_bytes: usize,
    pub selected_patch_bytes: usize,
    pub total_files: usize,
    pub selected_files: usize,
}
