use super::review_comment::{ReviewComment, ReviewCoverage};
use crate::error::{GbMcpError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeadGuard {
    Preflight,
    Atomic,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateReviewComment {
    pub body: String,
    pub path: String,
    pub commit: String,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub head_guard: HeadGuard,
}

impl CreateReviewComment {
    pub fn validate(&self) -> Result<()> {
        if self.head_guard != HeadGuard::Preflight {
            return Err(GbMcpError::Unsupported("Standard Web cannot provide an atomic head guard; explicitly select preflight only if its race window is acceptable".into()));
        }
        if self.body.trim().is_empty() || self.path.is_empty() {
            return Err(GbMcpError::InvalidInput(
                "body and path must not be empty".into(),
            ));
        }
        if self.body != self.body.trim() || self.path != self.path.trim() {
            return Err(GbMcpError::Unsupported("The Web form cannot preserve leading/trailing body or path whitespace; no write was sent".into()));
        }
        if self
            .body
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
            || self.path.chars().any(char::is_control)
        {
            return Err(GbMcpError::Unsupported("The Web verification contract does not support control characters or CR line endings; no write was sent".into()));
        }
        if self.path.starts_with('/')
            || self
                .path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err(GbMcpError::InvalidInput(
                "path must be a repository-relative Git path".into(),
            ));
        }
        if self.commit.len() != 40
            || !self
                .commit
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(GbMcpError::InvalidInput(
                "commit must be a full lowercase 40-character SHA-1".into(),
            ));
        }
        let line = match (self.old_line, self.new_line) {
            (Some(line), None) | (None, Some(line)) => line,
            _ => {
                return Err(GbMcpError::InvalidInput(
                    "Supply exactly one of old_line or new_line".into(),
                ))
            }
        };
        if line == 0 || line > i32::MAX as u64 {
            return Err(GbMcpError::InvalidInput(
                "The selected line must be between 1 and 2147483647".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPostVerification {
    pub head_guard: HeadGuard,
    pub atomic_head_guard: bool,
    pub raw_body_verified: bool,
    pub commit_page_verified: bool,
    pub pr_page_membership_verified: bool,
    pub stored_pr_association_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostedReviewComment {
    pub comment: ReviewComment,
    pub coverage: ReviewCoverage,
    pub verification: ReviewPostVerification,
}
