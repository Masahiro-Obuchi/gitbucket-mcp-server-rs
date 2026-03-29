use serde::{Deserialize, Serialize};

use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Label {
    pub name: String,
    pub color: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub user: Option<User>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub assignees: Vec<User>,
    pub html_url: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub comments: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssue {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_issue() {
        let json = r#"{
            "number": 42,
            "title": "Bug report",
            "body": "Something is broken",
            "state": "open",
            "user": {"login": "reporter"},
            "labels": [{"name": "bug", "color": "d73a4a"}],
            "assignees": [{"login": "developer"}],
            "html_url": "https://gitbucket.example.com/owner/repo/issues/42",
            "created_at": "2024-01-01T00:00:00Z",
            "comments": 3
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.number, 42);
        assert_eq!(issue.title, "Bug report");
        assert_eq!(issue.state, "open");
        assert_eq!(issue.labels.len(), 1);
        assert_eq!(issue.labels[0].name, "bug");
        assert_eq!(issue.assignees.len(), 1);
    }

    #[test]
    fn test_deserialize_issue_minimal() {
        let json = r#"{
            "number": 1,
            "title": "Test",
            "state": "closed"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.number, 1);
        assert!(issue.labels.is_empty());
        assert!(issue.assignees.is_empty());
    }

    #[test]
    fn test_serialize_create_issue() {
        let create = CreateIssue {
            title: "New bug".to_string(),
            body: Some("Description here".to_string()),
            labels: Some(vec!["bug".to_string()]),
            assignees: None,
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["title"], "New bug");
        assert_eq!(json["labels"][0], "bug");
        assert!(json.get("assignees").is_none());
    }

    #[test]
    fn test_serialize_update_issue() {
        let update = UpdateIssue {
            state: Some("closed".to_string()),
            title: None,
            body: None,
        };

        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["state"], "closed");
        assert!(json.get("title").is_none());
    }
}
