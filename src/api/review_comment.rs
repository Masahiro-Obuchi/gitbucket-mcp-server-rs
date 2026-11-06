//! Read-only operation service for the verified standard Web contract.
use super::{
    client::GitBucketClient,
    profile::{Operation, Route},
    review_html, url_safety,
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        pull_request::PullRequest,
        review_comment::{self, ReviewCommentList, ReviewCommentLookup, ReviewCoverage},
    },
};

pub(super) fn revision(pr: &PullRequest, number: u64) -> Result<(String, String)> {
    let sha = |value: Option<&str>| {
        value
            .filter(|v| v.len() == 40 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_owned)
    };
    if pr.number != number {
        return Err(GbMcpError::Other(
            "GitBucket returned a different pull request".into(),
        ));
    }
    match (
        sha(pr.head.as_ref().and_then(|h| h.sha.as_deref())),
        sha(pr.base.as_ref().and_then(|b| b.sha.as_deref())),
    ) {
        (Some(head), Some(base)) => Ok((head, base)),
        _ => Err(GbMcpError::Unsupported(
            "Review reads require explicit PR head/base commit IDs for before/after observation"
                .into(),
        )),
    }
}

impl GitBucketClient {
    pub async fn list_pull_request_review_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<ReviewCommentList> {
        url_safety::repository(owner, repo)?;
        if number == 0 {
            return Err(GbMcpError::InvalidInput(
                "pull_number must be positive".into(),
            ));
        }
        if self.operation_route(Operation::ReviewCommentRead) != Route::Web {
            return Err(GbMcpError::Unsupported("Review comment reads require GITBUCKET_API_PROFILE=standard; no speculative REST or Web request was sent".into()));
        }
        if self.web_credentials().is_none() {
            return Err(GbMcpError::Authentication(
                "Review comment reads require GITBUCKET_USERNAME and GITBUCKET_PASSWORD",
            ));
        }
        let before = self.get_pull_request(owner, repo, number).await?;
        let (head, base) = revision(&before, number)?;
        let session = self.web_session().await?;
        let (page, page_url) = session
            .pull_request_conversation(owner, repo, number)
            .await?;
        let after = self.get_pull_request(owner, repo, number).await?;
        revision(&after, number)?;
        if before != after {
            return Err(GbMcpError::ReadConflict("Pull request metadata changed while reading review comments; no partial result was returned. Read again to observe the new page."));
        }
        let web_root = self
            .base_url()
            .strip_suffix("/api/v3")
            .expect("normalized API suffix");
        Ok(ReviewCommentList {
            comments: review_html::parse(&page, &page_url, web_root)?,
            coverage: ReviewCoverage::web(Some(head), Some(base)),
        })
    }

    pub async fn get_pull_request_review_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        id: &str,
    ) -> Result<ReviewCommentLookup> {
        review_comment::storage_id(id)?;
        Ok(self
            .list_pull_request_review_comments(owner, repo, number)
            .await?
            .lookup(id))
    }
}
