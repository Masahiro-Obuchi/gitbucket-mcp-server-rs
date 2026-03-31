pub fn error(message: impl Into<String>) -> String {
    message.into()
}

pub fn required_trimmed(value: &str, field: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(error(format!("{field} must not be empty")));
    }
    Ok(trimmed.to_string())
}

pub fn optional_trimmed(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string())
}

pub fn list_state(value: Option<String>) -> std::result::Result<Option<String>, String> {
    match value {
        Some(state) => {
            let normalized = required_trimmed(&state, "state")?;
            match normalized.as_str() {
                "open" | "closed" | "all" => Ok(Some(normalized)),
                _ => Err(error("state must be one of: open, closed, all")),
            }
        }
        None => Ok(None),
    }
}

pub fn issue_state(value: Option<String>) -> std::result::Result<Option<String>, String> {
    match value {
        Some(state) => {
            let normalized = required_trimmed(&state, "state")?;
            match normalized.as_str() {
                "open" | "closed" => Ok(Some(normalized)),
                _ => Err(error("state must be one of: open, closed")),
            }
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_trimmed_rejects_blank() {
        let err = required_trimmed("   ", "owner").unwrap_err();
        assert_eq!(err, "owner must not be empty");
    }

    #[test]
    fn test_required_trimmed_trims_value() {
        let value = required_trimmed("  repo  ", "repo").unwrap();
        assert_eq!(value, "repo");
    }

    #[test]
    fn test_list_state_rejects_invalid_value() {
        let err = list_state(Some("draft".to_string())).unwrap_err();
        assert_eq!(err, "state must be one of: open, closed, all");
    }

    #[test]
    fn test_issue_state_rejects_invalid_value() {
        let err = issue_state(Some("all".to_string())).unwrap_err();
        assert_eq!(err, "state must be one of: open, closed");
    }
}
