use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    error::GbMcpError,
    models::operation::{OperationOutcome, OutcomeStatus},
};

pub type ToolResult = Result<CallToolResult, rmcp::ErrorData>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolErrorPayload {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<ToolErrorPayload>>,
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
    Ok(CallToolResult::structured_error(json!(gb_error_payload(
        error
    ))))
}

fn gb_error_payload(error: GbMcpError) -> ToolErrorPayload {
    match error {
        err @ GbMcpError::Timeout { .. } => payload("timeout_error", err.to_string(), None),
        err @ GbMcpError::Cancelled => payload("cancelled_error", err.to_string(), None),
        err @ GbMcpError::ResponseTooLarge { .. } => {
            payload("response_too_large", err.to_string(), None)
        }
        GbMcpError::InvalidInput(message) => payload("validation_error", message, None),
        GbMcpError::Unsupported(message) => payload("unsupported_error", message, None),
        GbMcpError::OperationFailed { mut outcome, cause } => {
            if outcome.status == OutcomeStatus::Verified {
                outcome.status = OutcomeStatus::Unknown;
            }
            let cause = gb_error_payload(*cause);
            let kind = match outcome.status {
                OutcomeStatus::Partial => "operation_partial",
                OutcomeStatus::Unknown => "operation_unknown",
                OutcomeStatus::NotApplied => "operation_not_applied",
                OutcomeStatus::NotVerified => "operation_not_verified",
                OutcomeStatus::Verified => "operation_unknown", // an error is never a verified success
            };
            let message = format!("{outcome}. Cause: {}", cause.message);
            ToolErrorPayload {
                kind: kind.into(),
                message,
                status: cause.status,
                operation: Some(*outcome),
                cause: Some(Box::new(cause)),
            }
        }
        err @ GbMcpError::Authentication(_) => {
            payload("authentication_error", err.to_string(), None)
        }
        GbMcpError::Config(message) => payload("config_error", message, None),
        GbMcpError::Api { status, message } => payload(
            "api_error",
            if message.trim().is_empty() {
                format!("GitBucket API returned HTTP {}", status)
            } else {
                format!("API error ({}): {}", status, message)
            },
            Some(status),
        ),
        err @ GbMcpError::Http(_) => payload("http_error", err.to_string(), None),
        err @ GbMcpError::Json(_) => payload("json_error", err.to_string(), None),
        GbMcpError::UrlParse(error) => payload(
            "url_parse_error",
            format!("URL parse error: {}", error),
            None,
        ),
        GbMcpError::Other(message) => payload("other_error", message, None),
    }
}

fn error_payload(kind: &'static str, message: String, status: Option<u16>) -> ToolResult {
    Ok(CallToolResult::structured_error(json!(payload(
        kind, message, status
    ))))
}

fn payload(kind: &'static str, message: String, status: Option<u16>) -> ToolErrorPayload {
    ToolErrorPayload {
        kind: kind.into(),
        message,
        status,
        operation: None,
        cause: None,
    }
}
