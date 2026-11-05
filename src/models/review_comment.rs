//! Review observations are separate from ordinary Issue comments and stored truth.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCommentKind {
    Commit,
    File,
    Line,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewBodyFormat {
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSource {
    GitbucketWebConversation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAssociation {
    ObservedOnPullRequestPage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewComment {
    /// Repository-scoped storage namespace; never an IssueComment ID.
    pub id: String,
    pub storage_id: u64,
    pub kind: ReviewCommentKind,
    pub body_raw: Option<String>,
    pub body_html: String,
    pub body_format: ReviewBodyFormat,
    pub author: String,
    pub created_at: Option<String>,
    pub created_at_display: Option<String>,
    pub updated_at: Option<String>,
    pub path: Option<String>,
    pub observed_old_line: Option<u64>,
    pub observed_new_line: Option<u64>,
    /// Only an explicit commit link supplies this; CSS is not a commit ID.
    pub current_commit: Option<String>,
    pub original_commit: Option<String>,
    pub original_old_line: Option<u64>,
    pub original_new_line: Option<u64>,
    pub position_verified: bool,
    pub association: ReviewAssociation,
    pub source: ReviewSource,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCoverage {
    pub scope: ReviewSource,
    /// Completeness is relative to stored PR history, not just this page.
    pub complete: bool,
    pub body_raw_complete: bool,
    pub historical_complete: bool,
    pub association_verified: bool,
    pub observed_head: Option<String>,
    pub observed_base: Option<String>,
    pub missing_reasons: Vec<String>,
}

impl ReviewCoverage {
    pub fn web(observed_head: Option<String>, observed_base: Option<String>) -> Self {
        Self {
            scope: ReviewSource::GitbucketWebConversation,
            complete: false,
            body_raw_complete: false,
            historical_complete: false,
            association_verified: false,
            observed_head,
            observed_base,
            missing_reasons: [
                "web_page_is_not_stored_history",
                "raw_markdown_not_available",
                "stored_pr_association_not_exposed",
                "original_positions_and_update_times_not_exposed",
                "positions_not_verified_against_diff",
                "reads_are_not_atomic",
            ]
            .map(str::to_owned)
            .into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommentList {
    pub comments: Vec<ReviewComment>,
    pub coverage: ReviewCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLookupStatus {
    Found,
    NotObserved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommentLookup {
    pub status: ReviewLookupStatus,
    pub comment: Option<ReviewComment>,
    pub coverage: ReviewCoverage,
}

impl ReviewCommentList {
    pub fn lookup(self, id: &str) -> ReviewCommentLookup {
        let comment = self.comments.into_iter().find(|comment| comment.id == id);
        ReviewCommentLookup {
            status: if comment.is_some() {
                ReviewLookupStatus::Found
            } else {
                ReviewLookupStatus::NotObserved
            },
            comment,
            coverage: self.coverage,
        }
    }
}

pub(crate) fn storage_id(id: &str) -> crate::error::Result<u64> {
    id.strip_prefix("commit:")
        .and_then(positive_decimal)
        .ok_or_else(|| {
            crate::error::GbMcpError::InvalidInput(
                "comment_id must be commit:<positive decimal ID>, without leading zeros".into(),
            )
        })
}

pub(crate) fn positive_decimal(value: &str) -> Option<u64> {
    if value.starts_with('0') || !value.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    value.parse::<u64>().ok().filter(|value| *value > 0)
}
