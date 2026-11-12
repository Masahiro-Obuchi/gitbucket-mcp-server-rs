//! Observations from a write operation, independent of HTTP and MCP.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Verified,
    Partial,
    Unknown,
    NotApplied,
    NotVerified,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatus {
    NotAttempted,
    Unknown,
    Rejected,
    Verified,
    Unchanged,
    Mismatched,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldIntent {
    Requested,
    Preserved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldOutcome {
    pub field: String,
    pub intent: FieldIntent,
    pub status: FieldStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriteRoute {
    Rest,
    Web,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Issue,
    PullRequest,
    Label,
    Milestone,
    Repository,
    IssueComment,
    IssueLabels,
    PullRequestComment,
    PullRequestReviewComment,
}

/// Locators only: never include credentials, URLs, titles or body values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationTarget {
    pub resource: ResourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_numbers: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u64>,
}

impl OperationTarget {
    pub fn new(resource: ResourceKind, owner: &str, repo: &str) -> Self {
        Self {
            resource,
            owner: (!owner.is_empty()).then(|| owner.into()),
            repo: repo.into(),
            number: None,
            parent_number: None,
            name: None,
            requested_name: None,
            candidate_numbers: Vec::new(),
            observed_number: None,
            commit: None,
            result_commit: None,
            path: None,
            old_line: None,
            new_line: None,
        }
    }

    pub fn numbered(mut self, number: u64) -> Self {
        self.number = Some(number);
        self
    }

    pub fn parented(mut self, number: u64) -> Self {
        self.parent_number = Some(number);
        self
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn renamed(mut self, next: &str) -> Self {
        self.requested_name = Some(next.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationOutcome {
    pub name: String,
    pub status: OutcomeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<WriteRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<OperationTarget>,
    pub fields: Vec<FieldOutcome>,
}

impl std::fmt::Display for OperationOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:?}; inspect the resource before retrying",
            self.name, self.status
        )
    }
}
