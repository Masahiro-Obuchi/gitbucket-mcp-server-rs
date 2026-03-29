use serde::{Deserialize, Serialize};

use super::repository::Repository;
use super::user::User;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequestHead {
    pub label: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: Option<String>,
    pub repo: Option<Repository>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub user: Option<User>,
    pub html_url: Option<String>,
    pub head: Option<PullRequestHead>,
    pub base: Option<PullRequestHead>,
    pub merged: Option<bool>,
    pub mergeable: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePullRequest {
    pub title: String,
    pub head: String,
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePullRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeResult {
    pub sha: Option<String>,
    pub merged: Option<bool>,
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_pull_request() {
        let json = r#"{
            "number": 10,
            "title": "Add feature X",
            "body": "This PR adds feature X",
            "state": "open",
            "user": {"login": "developer"},
            "html_url": "https://gitbucket.example.com/owner/repo/pull/10",
            "head": {
                "label": "developer:feature-x",
                "ref": "feature-x",
                "sha": "abc123"
            },
            "base": {
                "label": "owner:main",
                "ref": "main",
                "sha": "def456"
            },
            "merged": false,
            "mergeable": true,
            "created_at": "2024-01-01T00:00:00Z"
        }"#;

        let pr: PullRequest = serde_json::from_str(json).unwrap();
        assert_eq!(pr.number, 10);
        assert_eq!(pr.title, "Add feature X");
        assert_eq!(pr.state, "open");
        assert_eq!(pr.head.as_ref().unwrap().ref_name, "feature-x");
        assert_eq!(pr.base.as_ref().unwrap().ref_name, "main");
        assert_eq!(pr.merged, Some(false));
    }

    #[test]
    fn test_serialize_create_pull_request() {
        let create = CreatePullRequest {
            title: "New feature".to_string(),
            head: "feature-branch".to_string(),
            base: "main".to_string(),
            body: Some("PR description".to_string()),
        };

        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["title"], "New feature");
        assert_eq!(json["head"], "feature-branch");
        assert_eq!(json["base"], "main");
    }

    #[test]
    fn test_serialize_merge_pull_request_minimal() {
        let merge = MergePullRequest {
            commit_message: None,
            sha: None,
            merge_method: None,
        };

        let json = serde_json::to_value(&merge).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.is_empty());
    }

    #[test]
    fn test_deserialize_merge_result() {
        let json = r#"{
            "sha": "merged123",
            "merged": true,
            "message": "Pull Request successfully merged"
        }"#;

        let result: MergeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.merged, Some(true));
        assert_eq!(result.sha, Some("merged123".to_string()));
    }
}
