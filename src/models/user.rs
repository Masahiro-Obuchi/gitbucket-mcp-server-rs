use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub login: String,
    pub email: Option<String>,
    #[serde(rename = "type")]
    pub user_type: Option<String>,
    pub site_admin: Option<bool>,
    pub created_at: Option<String>,
    pub avatar_url: Option<String>,
    pub url: Option<String>,
    pub html_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_user_full() {
        let json = r#"{
            "login": "testuser",
            "email": "test@example.com",
            "type": "User",
            "site_admin": false,
            "created_at": "2024-01-01T00:00:00Z",
            "avatar_url": "https://gitbucket.example.com/testuser/_avatar",
            "url": "https://gitbucket.example.com/api/v3/users/testuser",
            "html_url": "https://gitbucket.example.com/testuser"
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.login, "testuser");
        assert_eq!(user.email, Some("test@example.com".to_string()));
        assert_eq!(user.user_type, Some("User".to_string()));
        assert_eq!(user.site_admin, Some(false));
    }

    #[test]
    fn test_deserialize_user_minimal() {
        let json = r#"{"login": "admin"}"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.login, "admin");
        assert!(user.email.is_none());
        assert!(user.user_type.is_none());
    }

    #[test]
    fn test_serialize_user() {
        let user = User {
            login: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            user_type: Some("User".to_string()),
            site_admin: Some(false),
            created_at: None,
            avatar_url: None,
            url: None,
            html_url: None,
        };

        let json = serde_json::to_value(&user).unwrap();
        assert_eq!(json["login"], "testuser");
        assert_eq!(json["type"], "User");
    }
}
