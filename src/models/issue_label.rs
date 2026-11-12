use serde::{Deserialize, Serialize};

/// Exact label names for an Issue/PR assignment change. Unknown names are
/// created as repository labels by GitBucket's REST endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LabelNames {
    pub labels: Vec<String>,
}
