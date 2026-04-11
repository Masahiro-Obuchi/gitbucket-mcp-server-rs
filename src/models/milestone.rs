use serde::{Deserialize, Serialize};

use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Milestone {
    pub number: u64,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub due_on: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub creator: Option<User>,
    #[serde(default)]
    pub open_issues: Option<u64>,
    #[serde(default)]
    pub closed_issues: Option<u64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMilestone {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMilestone {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_milestone() {
        let json = r#"{
            "number": 7,
            "title": "v1.0",
            "state": "open",
            "description": "First release",
            "due_on": "2026-04-01T00:00:00Z",
            "open_issues": 3,
            "closed_issues": 1,
            "creator": {"login": "alice"}
        }"#;

        let milestone: Milestone = serde_json::from_str(json).unwrap();
        assert_eq!(milestone.number, 7);
        assert_eq!(milestone.title, "v1.0");
        assert_eq!(milestone.state, "open");
        assert_eq!(milestone.description.as_deref(), Some("First release"));
        assert_eq!(milestone.open_issues, Some(3));
        assert_eq!(
            milestone
                .creator
                .as_ref()
                .map(|creator| creator.login.as_str()),
            Some("alice")
        );
    }

    #[test]
    fn test_serialize_create_milestone() {
        let create = CreateMilestone {
            title: "v1.0".to_string(),
            description: Some("First release".to_string()),
            due_on: Some("2026-04-01".to_string()),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["title"], "v1.0");
        assert_eq!(json["description"], "First release");
        assert_eq!(json["due_on"], "2026-04-01");
    }

    #[test]
    fn test_serialize_update_milestone() {
        let update = UpdateMilestone {
            title: Some("v1.1".to_string()),
            description: Some(String::new()),
            due_on: None,
            state: Some("closed".to_string()),
        };

        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["title"], "v1.1");
        assert_eq!(json["description"], "");
        assert_eq!(json["state"], "closed");
        assert!(json.get("due_on").is_none());
    }
}
