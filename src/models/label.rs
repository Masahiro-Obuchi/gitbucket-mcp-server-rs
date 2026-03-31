use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateLabel {
    pub name: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_label() {
        let json = r#"{
            "name": "bug",
            "color": "fc2929",
            "description": "Broken behavior",
            "url": "https://gitbucket.example.com/api/v3/repos/alice/project/labels/bug"
        }"#;

        let label: Label = serde_json::from_str(json).unwrap();
        assert_eq!(label.name, "bug");
        assert_eq!(label.color.as_deref(), Some("fc2929"));
        assert_eq!(label.description.as_deref(), Some("Broken behavior"));
    }

    #[test]
    fn test_deserialize_label_minimal() {
        let json = r#"{"name": "needs-review"}"#;

        let label: Label = serde_json::from_str(json).unwrap();
        assert_eq!(label.name, "needs-review");
        assert!(label.color.is_none());
        assert!(label.description.is_none());
    }

    #[test]
    fn test_serialize_create_label() {
        let create = CreateLabel {
            name: "bug".to_string(),
            color: "fc2929".to_string(),
            description: Some("Broken behavior".to_string()),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["name"], "bug");
        assert_eq!(json["color"], "fc2929");
        assert_eq!(json["description"], "Broken behavior");
    }
}
