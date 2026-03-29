use thiserror::Error;

#[derive(Error, Debug)]
pub enum GbMcpError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GbMcpError>;

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
