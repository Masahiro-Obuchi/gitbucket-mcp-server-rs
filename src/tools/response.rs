use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::GbMcpError;

pub type ToolResult = Result<CallToolResult, rmcp::ErrorData>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolErrorPayload {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

pub fn success<T: Serialize>(value: &T) -> ToolResult {
    let value = serde_json::to_value(value).map_err(|e| {
        rmcp::ErrorData::internal_error(
            format!("Failed to serialize structured content: {}", e),
            None,
        )
    })?;
    Ok(CallToolResult::structured(value))
}

pub fn success_list<T: Serialize>(field: &'static str, value: &T) -> ToolResult {
    let value = serde_json::to_value(value).map_err(|e| {
        rmcp::ErrorData::internal_error(
            format!("Failed to serialize structured content: {}", e),
            None,
        )
    })?;
    let mut object = Map::new();
    object.insert(field.to_string(), value);
    Ok(CallToolResult::structured(Value::Object(object)))
}

pub fn validation_error(message: impl Into<String>) -> ToolResult {
    error_payload("validation_error", message.into(), None)
}

pub fn internal_error(message: impl Into<String>) -> ToolResult {
    error_payload("internal_error", message.into(), None)
}

pub fn from_gb_error(error: GbMcpError) -> ToolResult {
    match error {
        err @ GbMcpError::UpdateNotVerified { .. } => {
            error_payload("update_not_verified", err.to_string(), None)
        }
        err @ GbMcpError::WriteOutcomeUnknown { .. } => {
            error_payload("outcome_unknown", err.to_string(), None)
        }
        GbMcpError::Config(message) => error_payload("config_error", message, None),
        GbMcpError::Api { status, message } => error_payload(
            "api_error",
            if message.trim().is_empty() {
                format!("GitBucket API returned HTTP {}", status)
            } else {
                format!("API error ({}): {}", status, message)
            },
            Some(status),
        ),
        GbMcpError::Http(error) => {
            error_payload("http_error", format!("HTTP error: {}", error), None)
        }
        GbMcpError::Json(error) => {
            error_payload("json_error", format!("JSON error: {}", error), None)
        }
        GbMcpError::UrlParse(error) => error_payload(
            "url_parse_error",
            format!("URL parse error: {}", error),
            None,
        ),
        GbMcpError::Other(message) => error_payload("other_error", message, None),
    }
}

fn error_payload(kind: &'static str, message: String, status: Option<u16>) -> ToolResult {
    Ok(CallToolResult::structured_error(json!(ToolErrorPayload {
        kind: kind.to_string(),
        message,
        status,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unverified_update_is_a_tool_error() {
        let result = from_gb_error(GbMcpError::UpdateNotVerified {
            fields: vec!["body"],
        })
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["kind"], "update_not_verified");
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("before retrying"));
    }

    #[test]
    fn test_unknown_write_outcome_is_a_tool_error() {
        let result = from_gb_error(GbMcpError::WriteOutcomeUnknown {
            cause: Box::new(GbMcpError::Api {
                status: 500,
                message: "response unavailable".into(),
            }),
        })
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["kind"], "outcome_unknown");
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("already have been applied"));
    }
}
