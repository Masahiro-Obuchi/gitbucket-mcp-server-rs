//! One-shot, preflight-guarded review creation with independent post-write reads.
use super::{
    client::GitBucketClient,
    outcome::Operation,
    profile::{Operation as Capability, Route},
    review_comment::revision,
    review_html, review_patch, url_safety,
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        operation::{OperationTarget, ResourceKind},
        review_comment::{ReviewBodyFormat, ReviewComment, ReviewCoverage},
        review_post::{
            CreateReviewComment, HeadGuard, PostedReviewComment, ReviewPostVerification,
        },
    },
};
use std::collections::HashSet;

fn same_position(comment: &ReviewComment, request: &CreateReviewComment, author: &str) -> bool {
    comment.path.as_deref() == Some(&request.path)
        && comment.observed_old_line == request.old_line
        && comment.observed_new_line == request.new_line
        && comment.author == author
}

impl GitBucketClient {
    pub async fn add_pull_request_review_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        change: &CreateReviewComment,
    ) -> Result<PostedReviewComment> {
        url_safety::repository(owner, repo)?;
        change.validate()?;
        if number == 0 {
            return Err(GbMcpError::InvalidInput(
                "pull_number must be positive".into(),
            ));
        }
        let route = self.operation_route(Capability::ReviewCommentCreate);
        if route != Route::Web {
            return Err(GbMcpError::Unsupported("Review posting requires the explicitly selected standard profile; no REST fallback is attempted".into()));
        }
        let credentials = self.web_credentials().ok_or(GbMcpError::Authentication(
            "Review posting requires matching PAT and Web credentials",
        ))?;
        let before = self.get_pull_request(owner, repo, number).await?;
        let (head, base) = revision(&before, number)?;
        if before.state != "open" || head != change.commit {
            return Err(GbMcpError::ReadConflict("Review posting requires an open PR and its exact current head commit; no write was sent"));
        }
        let session = self.web_session().await?;
        let patch = session.review_patch(owner, repo, &base, &head).await?;
        let diff = review_patch::select(&patch, change)?;
        let web_root = self
            .base_url()
            .strip_suffix("/api/v3")
            .expect("normalized API suffix");
        let (page, pr_url) = session
            .pull_request_conversation(owner, repo, number)
            .await?;
        let baseline: HashSet<_> = review_html::parse(&page, &pr_url, web_root)?
            .into_iter()
            .map(|c| c.storage_id)
            .collect();
        session.prepare_review_write().await?;
        let final_pr = self.get_pull_request(owner, repo, number).await?;
        if final_pr != before {
            return Err(GbMcpError::ReadConflict(
                "PR metadata changed during posting preflight; no write was sent",
            ));
        }

        let mut target = OperationTarget::new(ResourceKind::PullRequestReviewComment, owner, repo)
            .parented(number);
        target.commit = Some(change.commit.clone());
        target.path = Some(change.path.clone());
        target.old_line = change.old_line;
        target.new_line = change.new_line;
        let operation = Operation::new(
            "add_pull_request_review_comment",
            target,
            route,
            &[
                ("body", true),
                ("path", true),
                ("position", true),
                ("commit", true),
                ("pr_page_membership", true),
            ],
        );
        let post = async {
            let response = session
                .create_review_comment(owner, repo, number, change, &diff)
                .await?;
            let id = review_html::posted_id(&response)?;
            if baseline.contains(&id) {
                operation.candidates(vec![id]);
                return Err(GbMcpError::Other(
                    "The creation response reused an already observed comment ID; inspect before retrying".into(),
                ));
            }
            operation.identity(id)?;
            let returned = review_html::posted_card(&response, &pr_url, web_root)?;
            if !same_position(&returned, change, &credentials.username) {
                return Err(GbMcpError::Other(
                    "Created review response did not identify the requested position/author".into(),
                ));
            }
            let raw_page = session.own_review_body(owner, repo, id).await?;
            let raw = review_html::own_raw_body(&raw_page, id)?;
            operation.verify(&[("body", raw == change.body)])?;
            let commit_page = session.review_commit_page(owner, repo, &head).await?;
            let stored = review_html::commit_comments(&commit_page, &pr_url, web_root, &head)?
                .into_iter()
                .find(|c| c.storage_id == id)
                .ok_or_else(|| {
                    GbMcpError::Other(
                        "Created review ID was not observed on the requested commit page".into(),
                    )
                })?;
            operation.verify(&[
                ("commit", true),
                ("path", stored.path.as_deref() == Some(&change.path)),
                (
                    "position",
                    same_position(&stored, change, &credentials.username),
                ),
            ])?;
            let (page, _) = session
                .pull_request_conversation(owner, repo, number)
                .await?;
            let mut observed = review_html::parse(&page, &pr_url, web_root)?
                .into_iter()
                .find(|c| c.storage_id == id)
                .ok_or_else(|| {
                    GbMcpError::Other(
                        "Created review ID was not observed on the PR page; its stored association is unknown".into(),
                    )
                })?;
            if !same_position(&observed, change, &credentials.username) {
                return Err(GbMcpError::Other(
                    "Created review position changed during verification".into(),
                ));
            }
            let after = self.get_pull_request(owner, repo, number).await?;
            if revision(&after, number)? != (head.clone(), base.clone()) || after.state != "open" {
                return Err(GbMcpError::ReadConflict(
                    "PR head/base/state changed after posting; the comment may already exist. Inspect its ID before any retry",
                ));
            }
            operation.verify(&[("pr_page_membership", true)])?;
            observed.body_raw = Some(raw);
            observed.body_format = ReviewBodyFormat::Markdown;
            observed.current_commit = Some(head.clone());
            observed.position_verified = true;
            Ok(PostedReviewComment {
                comment: observed,
                coverage: ReviewCoverage::web(Some(head), Some(base)),
                verification: ReviewPostVerification {
                    head_guard: HeadGuard::Preflight,
                    atomic_head_guard: false,
                    raw_body_verified: true,
                    commit_page_verified: true,
                    pr_page_membership_verified: true,
                    stored_pr_association_verified: false,
                },
            })
        };
        operation.run(post).await
    }
}
