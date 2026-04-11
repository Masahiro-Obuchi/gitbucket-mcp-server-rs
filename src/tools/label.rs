use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::label::{CreateLabel, UpdateLabel};
use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, validation_error, ToolResult};
use crate::tools::validation::{error, label_color, optional_trimmed, required_trimmed};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListLabelsParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLabelParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Label name")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateLabelParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Label name")]
    pub name: String,
    #[schemars(description = "6-digit label color, with or without #")]
    pub color: String,
    #[schemars(description = "Optional label description")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateLabelParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Current label name")]
    pub name: String,
    #[schemars(description = "New label name")]
    pub new_name: Option<String>,
    #[schemars(description = "New 6-digit label color, with or without #")]
    pub color: Option<String>,
    #[schemars(description = "New label description; pass an empty string to clear it")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteLabelParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Label name")]
    pub name: String,
}

#[derive(Debug, Serialize)]
struct DeleteLabelResult {
    deleted: bool,
    name: String,
}

#[tool_router(router = tool_router_label, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List labels for a GitBucket repository")]
    pub async fn list_labels(
        &self,
        Parameters(params): Parameters<ListLabelsParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self.client.list_labels(&owner, &repo).await {
            Ok(labels) => success(&labels),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Get details of a label in a GitBucket repository")]
    pub async fn get_label(&self, Parameters(params): Parameters<GetLabelParams>) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return validation_error(err),
        };

        match self.client.get_label(&owner, &repo, &name).await {
            Ok(label) => success(&label),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Create a new label in a GitBucket repository")]
    pub async fn create_label(
        &self,
        Parameters(params): Parameters<CreateLabelParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return validation_error(err),
        };
        let color = match label_color(&params.color) {
            Ok(color) => color,
            Err(err) => return validation_error(err),
        };

        let body = CreateLabel {
            name,
            color,
            description: optional_trimmed(params.description),
        };

        match self.client.create_label(&owner, &repo, &body).await {
            Ok(label) => success(&label),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Update a label in a GitBucket repository")]
    pub async fn update_label(
        &self,
        Parameters(params): Parameters<UpdateLabelParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return validation_error(err),
        };
        let new_name = match params.new_name {
            Some(new_name) => match required_trimmed(&new_name, "new_name") {
                Ok(new_name) => Some(new_name),
                Err(err) => return validation_error(err),
            },
            None => None,
        };
        let color = match params.color {
            Some(color) => match label_color(&color) {
                Ok(color) => Some(color),
                Err(err) => return validation_error(err),
            },
            None => None,
        };
        let description = params.description.map(|value| value.trim().to_string());

        if new_name.is_none() && color.is_none() && description.is_none() {
            return validation_error(error(
                "at least one of new_name, color, or description must be provided",
            ));
        }

        let body = UpdateLabel {
            new_name,
            color,
            description,
        };

        match self.client.update_label(&owner, &repo, &name, &body).await {
            Ok(label) => success(&label),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Delete a label from a GitBucket repository")]
    pub async fn delete_label(
        &self,
        Parameters(params): Parameters<DeleteLabelParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let name = match required_trimmed(&params.name, "name") {
            Ok(name) => name,
            Err(err) => return validation_error(err),
        };

        match self.client.delete_label(&owner, &repo, &name).await {
            Ok(()) => success(&DeleteLabelResult {
                deleted: true,
                name,
            }),
            Err(err) => from_gb_error(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Value;

    use super::*;
    use crate::api::client::GitBucketClient;
    use crate::server::GitBucketMcpServer;
    use crate::test_support::{MockApi, RecordedCall};
    use crate::tools::response::ToolErrorPayload;

    fn success_json(result: ToolResult) -> Value {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(false));
        result
            .structured_content
            .expect("expected structured content for success")
    }

    fn error_payload(result: ToolResult) -> ToolErrorPayload {
        let result = result.unwrap();
        assert_eq!(result.is_error, Some(true));
        serde_json::from_value(
            result
                .structured_content
                .expect("expected structured content for error"),
        )
        .expect("error payload should deserialize")
    }

    #[tokio::test]
    async fn test_create_label_rejects_invalid_color() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .create_label(Parameters(CreateLabelParams {
                owner: "alice".to_string(),
                repo: "project".to_string(),
                name: "bug".to_string(),
                color: "zzz".to_string(),
                description: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "color must be a 6-digit hex value like ff0000".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_update_label_rejects_empty_change_set() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .update_label(Parameters(UpdateLabelParams {
                owner: "alice".to_string(),
                repo: "project".to_string(),
                name: "bug".to_string(),
                new_name: None,
                color: None,
                description: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "at least one of new_name, color, or description must be provided"
                    .to_string(),
                status: None,
            }
        );
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn test_list_labels_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_labels(Parameters(ListLabelsParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result[0]["name"].as_str(), Some("bug"));
        match mock.calls().as_slice() {
            [RecordedCall::ListLabels { owner, repo }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_label_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .get_label(Parameters(GetLabelParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
                name: "  bug  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["name"].as_str(), Some("bug"));
        match mock.calls().as_slice() {
            [RecordedCall::GetLabel { owner, repo, name }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
                assert_eq!(name, "bug");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_label_passes_trimmed_fields_and_normalized_color() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .create_label(Parameters(CreateLabelParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
                name: "  needs-review  ".to_string(),
                color: "  #A1B2C3  ".to_string(),
                description: Some("  Needs extra review  ".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["name"].as_str(), Some("bug"));
        match mock.calls().as_slice() {
            [RecordedCall::CreateLabel { owner, repo, body }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
                assert_eq!(body.name, "needs-review");
                assert_eq!(body.color, "a1b2c3");
                assert_eq!(body.description.as_deref(), Some("Needs extra review"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_update_label_passes_trimmed_fields_and_normalized_color() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .update_label(Parameters(UpdateLabelParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
                name: "  needs review  ".to_string(),
                new_name: Some("  needs-review  ".to_string()),
                color: Some("  #A1B2C3  ".to_string()),
                description: Some("  Needs extra review  ".to_string()),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["name"].as_str(), Some("bug"));
        match mock.calls().as_slice() {
            [RecordedCall::UpdateLabel {
                owner,
                repo,
                name,
                body,
            }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
                assert_eq!(name, "needs review");
                assert_eq!(body.new_name.as_deref(), Some("needs-review"));
                assert_eq!(body.color.as_deref(), Some("a1b2c3"));
                assert_eq!(body.description.as_deref(), Some("Needs extra review"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_delete_label_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .delete_label(Parameters(DeleteLabelParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
                name: "  bug  ".to_string(),
            }))
            .await;

        let result = success_json(result);
        assert_eq!(result["deleted"].as_bool(), Some(true));
        assert_eq!(result["name"].as_str(), Some("bug"));
        match mock.calls().as_slice() {
            [RecordedCall::DeleteLabel { owner, repo, name }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
                assert_eq!(name, "bug");
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
