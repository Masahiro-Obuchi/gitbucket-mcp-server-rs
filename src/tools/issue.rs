use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::models::comment::CreateComment;
use crate::models::issue::{CreateIssue, UpdateIssue};
use crate::server::GitBucketMcpServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIssuesParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Filter by state: open, closed, or all (default: open)")]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue title")]
    pub title: String,
    #[schemars(description = "Issue body/description")]
    pub body: Option<String>,
    #[schemars(description = "Label names to assign")]
    pub labels: Option<Vec<String>>,
    #[schemars(description = "Usernames to assign")]
    pub assignees: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateIssueParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
    #[schemars(description = "New state: open or closed")]
    pub state: Option<String>,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New body")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIssueCommentsParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddIssueCommentParams {
    #[schemars(description = "Repository owner")]
    pub owner: String,
    #[schemars(description = "Repository name")]
    pub repo: String,
    #[schemars(description = "Issue number")]
    pub issue_number: u64,
    #[schemars(description = "Comment body text")]
    pub body: String,
}

#[tool_router(router = tool_router_issue, vis = "pub")]
impl GitBucketMcpServer {
    #[tool(description = "List issues in a GitBucket repository")]
    pub async fn list_issues(&self, Parameters(params): Parameters<ListIssuesParams>) -> String {
        match self
            .client
            .list_issues(&params.owner, &params.repo, params.state.as_deref())
            .await
        {
            Ok(issues) => serde_json::to_string_pretty(&issues)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Get details of a specific issue in a GitBucket repository")]
    pub async fn get_issue(&self, Parameters(params): Parameters<GetIssueParams>) -> String {
        match self
            .client
            .get_issue(&params.owner, &params.repo, params.issue_number)
            .await
        {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Create a new issue in a GitBucket repository")]
    pub async fn create_issue(&self, Parameters(params): Parameters<CreateIssueParams>) -> String {
        let body = CreateIssue {
            title: params.title,
            body: params.body,
            labels: params.labels,
            assignees: params.assignees,
        };
        match self
            .client
            .create_issue(&params.owner, &params.repo, &body)
            .await
        {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(
        description = "Update an issue in a GitBucket repository (change state, title, or body)"
    )]
    pub async fn update_issue(&self, Parameters(params): Parameters<UpdateIssueParams>) -> String {
        let body = UpdateIssue {
            state: params.state,
            title: params.title,
            body: params.body,
        };
        match self
            .client
            .update_issue(&params.owner, &params.repo, params.issue_number, &body)
            .await
        {
            Ok(issue) => serde_json::to_string_pretty(&issue)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "List comments on an issue in a GitBucket repository")]
    pub async fn list_issue_comments(
        &self,
        Parameters(params): Parameters<ListIssueCommentsParams>,
    ) -> String {
        match self
            .client
            .list_issue_comments(&params.owner, &params.repo, params.issue_number)
            .await
        {
            Ok(comments) => serde_json::to_string_pretty(&comments)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    #[tool(description = "Add a comment to an issue in a GitBucket repository")]
    pub async fn add_issue_comment(
        &self,
        Parameters(params): Parameters<AddIssueCommentParams>,
    ) -> String {
        let body = CreateComment { body: params.body };
        match self
            .client
            .add_issue_comment(&params.owner, &params.repo, params.issue_number, &body)
            .await
        {
            Ok(comment) => serde_json::to_string_pretty(&comment)
                .unwrap_or_else(|e| format!("Error serializing: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }
}
