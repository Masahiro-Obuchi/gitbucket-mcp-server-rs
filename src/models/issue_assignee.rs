use serde::{Deserialize, Serialize};

/// Canonical individual account logins for an Issue/PR assignment change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssigneeNames {
    pub assignees: Vec<String>,
}
