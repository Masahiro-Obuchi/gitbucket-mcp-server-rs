use serde::{Deserialize, Serialize};

use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Comment {
    pub id: u64,
    pub body: Option<String>,
    pub user: Option<User>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComment {
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_comment() {
        let json = r#"{
            "id": 1,
            "body": "Great work!",
            "user": {"login": "reviewer"},
            "created_at": "2024-01-01T12:00:00Z",
            "html_url": "https://gitbucket.example.com/owner/repo/issues/1#comment-1"
        }"#;

        let comment: Comment = serde_json::from_str(json).unwrap();
        assert_eq!(comment.id, 1);
        assert_eq!(comment.body, Some("Great work!".to_string()));
        assert_eq!(comment.user.unwrap().login, "reviewer");
    }

    #[test]
    fn test_serialize_create_comment() {
        let create = CreateComment {
            body: "This is a comment".to_string(),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["body"], "This is a comment");
    }
}
