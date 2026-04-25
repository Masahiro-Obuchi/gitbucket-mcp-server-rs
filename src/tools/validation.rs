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

pub fn repository_fields(owner: &str, repo: &str) -> std::result::Result<(String, String), String> {
    Ok((
        required_trimmed(owner, "owner")?,
        required_trimmed(repo, "repo")?,
    ))
}

pub fn required_optional_trimmed(
    value: Option<String>,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    value
        .map(|value| required_trimmed(&value, field))
        .transpose()
}

pub fn optional_trimmed(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string())
}

pub fn label_color(value: &str) -> std::result::Result<String, String> {
    let trimmed = required_trimmed(value, "color")?;
    let normalized = trimmed.strip_prefix('#').unwrap_or(&trimmed);
    if normalized.len() != 6 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(error("color must be a 6-digit hex value like ff0000"));
    }
    Ok(normalized.to_ascii_lowercase())
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
    fn test_repository_fields_trims_values() {
        let (owner, repo) = repository_fields(" owner ", " repo ").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_required_optional_trimmed_rejects_blank() {
        let err = required_optional_trimmed(Some(" ".to_string()), "title").unwrap_err();
        assert_eq!(err, "title must not be empty");
    }

    #[test]
    fn test_list_state_rejects_invalid_value() {
        let err = list_state(Some("draft".to_string())).unwrap_err();
        assert_eq!(err, "state must be one of: open, closed, all");
    }

    #[test]
    fn test_label_color_accepts_hash_prefix() {
        let color = label_color(" #A1B2C3 ").unwrap();
        assert_eq!(color, "a1b2c3");
    }

    #[test]
    fn test_label_color_rejects_invalid_value() {
        let err = label_color("zzz").unwrap_err();
        assert_eq!(err, "color must be a 6-digit hex value like ff0000");
    }

    #[test]
    fn test_label_color_rejects_multiple_hash_prefix() {
        let err = label_color("##A1B2C3").unwrap_err();
        assert_eq!(err, "color must be a 6-digit hex value like ff0000");
    }

    #[test]
    fn test_issue_state_rejects_invalid_value() {
        let err = issue_state(Some("all".to_string())).unwrap_err();
        assert_eq!(err, "state must be one of: open, closed");
    }
}
