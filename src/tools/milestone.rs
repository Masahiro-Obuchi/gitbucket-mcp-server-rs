use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::milestone::{CreateMilestone, UpdateMilestone};
use crate::server::GitBucketMcpServer;
use crate::tools::response::{from_gb_error, success, success_list, validation_error, ToolResult};
use crate::tools::validation::{error, issue_state, list_state, required_trimmed};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListMilestonesParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Filter by state: open, closed, or all (default: open)")]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetMilestoneParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Milestone number")]
    pub milestone_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateMilestoneParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Milestone title")]
    pub title: String,
    #[schemars(description = "Milestone description")]
    pub description: Option<String>,
    #[schemars(description = "Optional due date")]
    pub due_on: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateMilestoneParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Milestone number")]
    pub milestone_number: u64,
    #[schemars(description = "New milestone title")]
    pub title: Option<String>,
    #[schemars(description = "New milestone description; pass an empty string to clear it")]
    pub description: Option<String>,
    #[schemars(description = "New due date; pass an empty string to clear it")]
    pub due_on: Option<String>,
    #[schemars(description = "New state: open or closed")]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMilestoneParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Milestone number")]
    pub milestone_number: u64,
}

#[derive(Debug, Serialize)]
struct DeleteMilestoneResult {
    deleted: bool,
    number: u64,
}

#[tool_router(router = tool_router_milestone, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List milestones for a GitBucket repository")]
    pub async fn list_milestones(
        &self,
        Parameters(params): Parameters<ListMilestonesParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let state = match list_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .list_milestones(&owner, &repo, state.as_deref())
            .await
        {
            Ok(milestones) => success_list("milestones", &milestones),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Get details of a milestone in a GitBucket repository")]
    pub async fn get_milestone(
        &self,
        Parameters(params): Parameters<GetMilestoneParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .get_milestone(&owner, &repo, params.milestone_number)
            .await
        {
            Ok(milestone) => success(&milestone),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Create a milestone in a GitBucket repository")]
    pub async fn create_milestone(
        &self,
        Parameters(params): Parameters<CreateMilestoneParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let title = match required_trimmed(&params.title, "title") {
            Ok(title) => title,
            Err(err) => return validation_error(err),
        };

        let body = CreateMilestone {
            title,
            description: params
                .description
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            due_on: params
                .due_on
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };

        match self.client.create_milestone(&owner, &repo, &body).await {
            Ok(milestone) => success(&milestone),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Update a milestone in a GitBucket repository")]
    pub async fn update_milestone(
        &self,
        Parameters(params): Parameters<UpdateMilestoneParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };
        let title = match params.title {
            Some(title) => match required_trimmed(&title, "title") {
                Ok(title) => Some(title),
                Err(err) => return validation_error(err),
            },
            None => None,
        };
        let state = match issue_state(params.state) {
            Ok(state) => state,
            Err(err) => return validation_error(err),
        };
        let description = params.description.map(|value| value.trim().to_string());
        let due_on = params.due_on.map(|value| value.trim().to_string());

        if title.is_none() && description.is_none() && due_on.is_none() && state.is_none() {
            return validation_error(error(
                "at least one of title, description, due_on, or state must be provided",
            ));
        }

        let body = UpdateMilestone {
            title,
            description,
            due_on,
            state,
        };

        match self
            .client
            .update_milestone(&owner, &repo, params.milestone_number, &body)
            .await
        {
            Ok(milestone) => success(&milestone),
            Err(err) => from_gb_error(err),
        }
    }

    #[tool(description = "Delete a milestone from a GitBucket repository")]
    pub async fn delete_milestone(
        &self,
        Parameters(params): Parameters<DeleteMilestoneParams>,
    ) -> ToolResult {
        let owner = match required_trimmed(&params.owner, "owner") {
            Ok(owner) => owner,
            Err(err) => return validation_error(err),
        };
        let repo = match required_trimmed(&params.repo, "repo") {
            Ok(repo) => repo,
            Err(err) => return validation_error(err),
        };

        match self
            .client
            .delete_milestone(&owner, &repo, params.milestone_number)
            .await
        {
            Ok(()) => success(&DeleteMilestoneResult {
                deleted: true,
                number: params.milestone_number,
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
    async fn test_list_milestones_rejects_blank_owner() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .list_milestones(Parameters(ListMilestonesParams {
                owner: " ".to_string(),
                repo: "repo".to_string(),
                state: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "owner must not be empty".to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_update_milestone_rejects_empty_change_set() {
        let client = GitBucketClient::new("https://gitbucket.example.com", "test-token").unwrap();
        let server = GitBucketMcpServer::new(client);

        let result = server
            .update_milestone(Parameters(UpdateMilestoneParams {
                owner: "alice".to_string(),
                repo: "project".to_string(),
                milestone_number: 7,
                title: None,
                description: None,
                due_on: None,
                state: None,
            }))
            .await;

        assert_eq!(
            error_payload(result),
            ToolErrorPayload {
                kind: "validation_error".to_string(),
                message: "at least one of title, description, due_on, or state must be provided"
                    .to_string(),
                status: None,
            }
        );
    }

    #[tokio::test]
    async fn test_list_milestones_passes_trimmed_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .list_milestones(Parameters(ListMilestonesParams {
                owner: "  alice  ".to_string(),
                repo: "  project  ".to_string(),
                state: Some("closed".to_string()),
            }))
            .await;

        assert_eq!(
            success_json(result)["milestones"][0]["title"].as_str(),
            Some("v1.0")
        );
        match mock.calls().as_slice() {
            [RecordedCall::ListMilestones { owner, repo, state }] => {
                assert_eq!(owner, "alice");
                assert_eq!(repo, "project");
                assert_eq!(state.as_deref(), Some("closed"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_milestone_trims_optional_fields_and_serializes_response() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .create_milestone(Parameters(CreateMilestoneParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                title: "  v1.0  ".to_string(),
                description: Some("  first release  ".to_string()),
                due_on: Some(" 2026-04-01 ".to_string()),
            }))
            .await;

        assert_eq!(success_json(result)["title"].as_str(), Some("v1.0"));
        match mock.calls().as_slice() {
            [RecordedCall::CreateMilestone { owner, repo, body }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(body.title, "v1.0");
                assert_eq!(body.description.as_deref(), Some("first release"));
                assert_eq!(body.due_on.as_deref(), Some("2026-04-01"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_update_milestone_trims_fields_and_preserves_clear_values() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .update_milestone(Parameters(UpdateMilestoneParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                milestone_number: 7,
                title: Some("  v1.1  ".to_string()),
                description: Some("   ".to_string()),
                due_on: Some("".to_string()),
                state: Some("closed".to_string()),
            }))
            .await;

        assert_eq!(success_json(result)["title"].as_str(), Some("v1.0"));
        match mock.calls().as_slice() {
            [RecordedCall::UpdateMilestone {
                owner,
                repo,
                number,
                body,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
                assert_eq!(body.title.as_deref(), Some("v1.1"));
                assert_eq!(body.description.as_deref(), Some(""));
                assert_eq!(body.due_on.as_deref(), Some(""));
                assert_eq!(body.state.as_deref(), Some("closed"));
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }

    #[tokio::test]
    async fn test_delete_milestone_serializes_confirmation() {
        let mock = MockApi::default();
        let server = GitBucketMcpServer::new_with_api(Arc::new(mock.clone()));

        let result = server
            .delete_milestone(Parameters(DeleteMilestoneParams {
                owner: " owner ".to_string(),
                repo: " repo ".to_string(),
                milestone_number: 7,
            }))
            .await;

        let json = success_json(result);
        assert_eq!(json["deleted"].as_bool(), Some(true));
        assert_eq!(json["number"].as_u64(), Some(7));
        match mock.calls().as_slice() {
            [RecordedCall::DeleteMilestone {
                owner,
                repo,
                number,
            }] => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(*number, 7);
            }
            calls => panic!("unexpected calls: {calls:?}"),
        }
    }
}
