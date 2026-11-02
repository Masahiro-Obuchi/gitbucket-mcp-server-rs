use thiserror::Error;

#[derive(Error)]
pub enum GbMcpError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("HTTP request failed ({})", http_kind(.0))]
    Http(reqwest::Error),

    #[error("JSON response could not be decoded at line {}, column {}", .0.line(), .0.column())]
    Json(serde_json::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("{0}")]
    InvalidInput(String),

    #[error("{0}")]
    Unsupported(String),

    #[error("{operation} could not be verified for fields {fields:?}. Changes may already have been applied; inspect the resource before retrying.")]
    OperationNotVerified {
        operation: &'static str,
        fields: Vec<&'static str>,
    },

    #[error("{operation} outcome is unknown. Changes may already have been applied; inspect the resource before retrying. Cause: {cause}")]
    OperationOutcomeUnknown {
        operation: &'static str,
        #[source]
        cause: Box<GbMcpError>,
    },

    #[error("Pull request update could not be verified for fields {fields:?}. Changes may already have been applied; inspect the pull request before retrying.")]
    UpdateNotVerified { fields: Vec<&'static str> },

    #[error("Pull request update outcome is unknown. Changes may already have been applied; inspect the pull request before retrying. Cause: {cause}")]
    WriteOutcomeUnknown {
        #[source]
        cause: Box<GbMcpError>,
    },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GbMcpError>;

impl From<reqwest::Error> for GbMcpError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.without_url())
    }
}

impl From<serde_json::Error> for GbMcpError {
    fn from(error: serde_json::Error) -> Self {
        // Preserve the public variant and conversion without exposing the raw
        // response value through thiserror's automatic source chain.
        Self::Json(error)
    }
}

fn http_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection"
    } else if error.is_redirect() {
        "redirect rejected"
    } else if error.is_decode() {
        "decode"
    } else {
        "transport"
    }
}
impl std::fmt::Debug for GbMcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_display() {
        let err = GbMcpError::Config("missing URL".to_string());
        assert_eq!(err.to_string(), "Configuration error: missing URL");
    }

    #[test]
    fn test_api_error_display() {
        let err = GbMcpError::Api {
            status: 404,
            message: "Not Found".to_string(),
        };
        assert_eq!(err.to_string(), "API error (404): Not Found");
    }

    #[test]
    fn test_other_error_display() {
        let err = GbMcpError::Other("something went wrong".to_string());
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn test_json_error_from() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: GbMcpError = json_err.into();
        assert!(matches!(err, GbMcpError::Json(_)));
    }
}
