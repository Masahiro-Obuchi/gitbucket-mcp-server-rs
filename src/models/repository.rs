use serde::{Deserialize, Serialize};

use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: Option<String>,
    pub clone_url: Option<String>,
    #[serde(rename = "private")]
    pub is_private: bool,
    pub fork: bool,
    pub default_branch: Option<String>,
    pub owner: Option<User>,
    pub watchers_count: Option<u64>,
    pub forks_count: Option<u64>,
    pub open_issues_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRepository {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "private", skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_init: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Branch {
    pub name: String,
    pub commit: Option<BranchCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchCommit {
    pub sha: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_repository() {
        let json = r#"{
            "name": "my-repo",
            "full_name": "testuser/my-repo",
            "description": "A test repository",
            "html_url": "https://gitbucket.example.com/testuser/my-repo",
            "clone_url": "https://gitbucket.example.com/git/testuser/my-repo.git",
            "private": false,
            "fork": false,
            "default_branch": "main",
            "owner": {"login": "testuser"},
            "watchers_count": 5,
            "forks_count": 2,
            "open_issues_count": 3
        }"#;

        let repo: Repository = serde_json::from_str(json).unwrap();
        assert_eq!(repo.name, "my-repo");
        assert_eq!(repo.full_name, "testuser/my-repo");
        assert!(!repo.is_private);
        assert!(!repo.fork);
        assert_eq!(repo.default_branch, Some("main".to_string()));
        assert_eq!(repo.owner.unwrap().login, "testuser");
    }

    #[test]
    fn test_serialize_create_repository() {
        let create = CreateRepository {
            name: "new-repo".to_string(),
            description: Some("My new repo".to_string()),
            is_private: Some(true),
            auto_init: Some(true),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["name"], "new-repo");
        assert_eq!(json["private"], true);
        assert_eq!(json["auto_init"], true);
    }

    #[test]
    fn test_serialize_create_repository_minimal() {
        let create = CreateRepository {
            name: "new-repo".to_string(),
            description: None,
            is_private: None,
            auto_init: None,
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["name"], "new-repo");
        assert!(json.get("private").is_none());
        assert!(json.get("auto_init").is_none());
    }

    #[test]
    fn test_deserialize_branch() {
        let json = r#"{
            "name": "main",
            "commit": {"sha": "abc123def456"}
        }"#;

        let branch: Branch = serde_json::from_str(json).unwrap();
        assert_eq!(branch.name, "main");
        assert_eq!(branch.commit.unwrap().sha, "abc123def456");
    }
}
