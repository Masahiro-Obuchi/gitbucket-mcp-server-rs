use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListView {
    #[default]
    Summary,
    Full,
}

/// MCP response paging, independent of GitBucket's upstream page size.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListOptions {
    pub limit: Option<u64>,
    pub cursor: Option<String>,
    pub view: Option<ListView>,
}
