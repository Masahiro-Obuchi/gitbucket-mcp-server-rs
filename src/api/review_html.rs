//! Fail closed on unknown discussion structure; never infer Markdown or history.
use super::url_safety;
use crate::{
    error::{GbMcpError, Result},
    models::review_comment::{
        positive_decimal, ReviewAssociation, ReviewBodyFormat, ReviewComment, ReviewCommentKind,
        ReviewSource,
    },
};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use url::Url;

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("internal selector")
}
fn invalid() -> GbMcpError {
    GbMcpError::Other("Could not verify the GitBucket PR discussion structure; no empty or partial review result was returned".into())
}
fn one<'a>(mut nodes: impl Iterator<Item = ElementRef<'a>>) -> Result<ElementRef<'a>> {
    let node = nodes.next().ok_or_else(invalid)?;
    if nodes.next().is_some() {
        return Err(invalid());
    }
    Ok(node)
}

/// Markup inside a user-authored body cannot supply IDs, authors or page structure.
fn inside_body(node: ElementRef<'_>) -> bool {
    node.ancestors()
        .filter_map(ElementRef::wrap)
        .filter(|parent| parent.id() != node.id())
        .any(|parent| {
            parent
                .value()
                .id()
                .is_some_and(|id| id == "issueContent" || id.starts_with("commentContent-"))
                || parent
                    .value()
                    .classes()
                    .any(|class| class.starts_with("commit-commentContent-"))
        })
}

fn has_class(node: ElementRef<'_>, name: &str) -> bool {
    node.value().classes().any(|class| class == name)
}
fn same_target(actual: &Url, expected: &Url) -> bool {
    actual.origin() == expected.origin()
        && actual.path() == expected.path()
        && actual.query().is_none()
        && actual.fragment().is_none()
        && actual.username().is_empty()
        && actual.password().is_none()
}

pub(super) fn parse(page: &str, page_url: &Url, web_root: &str) -> Result<Vec<ReviewComment>> {
    let html = Html::parse_document(page);
    let nav = one(html
        .select(&selector("#pullreq-tab"))
        .filter(|node| !inside_body(*node)))?;
    one(html
        .select(&selector("#show-title"))
        .filter(|node| !inside_body(*node)))?;
    let list = one(html
        .select(&selector("#comment-list"))
        .filter(|node| !inside_body(*node)))?;
    one(list
        .select(&selector("#issueContent"))
        .filter(|node| !inside_body(*node)))?;
    let active = one(nav.select(&selector("li.active > a[href]")))?;
    let active_url = active
        .value()
        .attr("href")
        .and_then(|href| page_url.join(href).ok())
        .ok_or_else(invalid)?;
    if !same_target(&active_url, page_url) {
        return Err(invalid());
    }
    for suffix in ["commits", "files"] {
        let expected =
            Url::parse(&format!("{}/{suffix}", page_url.as_str())).map_err(|_| invalid())?;
        if !nav.select(&selector("a[href]")).any(|link| {
            link.value()
                .attr("href")
                .and_then(|href| page_url.join(href).ok())
                .is_some_and(|url| same_target(&url, &expected))
        }) {
            return Err(invalid());
        }
    }
    let badge = one(active.select(&selector(".badge")))?
        .text()
        .collect::<String>();
    let badge = badge.trim().parse::<usize>().map_err(|_| invalid())?;
    // Known top-level shapes include Issue comments/events and grouped line cards.
    for child in list.children().filter_map(ElementRef::wrap) {
        let known = child.value().name() == "script"
            || has_class(child, "issue-comment-box")
            || has_class(child, "commit-comment-box")
            || has_class(child, "discussion-item")
            || (has_class(child, "panel")
                && child
                    .select(&selector(".commit-comment-box"))
                    .any(|node| !inside_body(node)));
        if !known {
            return Err(invalid());
        }
    }

    let mut comments = Vec::new();
    let mut ids = HashSet::new();
    for card in list
        .select(&selector(".commit-comment-box"))
        .filter(|node| !inside_body(*node))
    {
        if card
            .ancestors()
            .filter_map(ElementRef::wrap)
            .filter(|parent| parent.id() != card.id())
            .any(|parent| has_class(parent, "commit-comment-box"))
        {
            return Err(invalid());
        }
        let (storage_id, kind) = if let Some(id) = card.value().id() {
            let id = id
                .strip_prefix("discussion_r")
                .and_then(positive_decimal)
                .ok_or_else(invalid)?;
            if card.value().attr("filename").is_none_or(str::is_empty) {
                return Err(invalid());
            }
            let kind = if card.value().attr("oldline").is_some()
                || card.value().attr("newline").is_some()
            {
                ReviewCommentKind::Line
            } else {
                ReviewCommentKind::File
            };
            (id, kind)
        } else {
            let class_ids: Vec<_> = card
                .value()
                .classes()
                .filter_map(|class| {
                    class
                        .strip_prefix("commit-comment-")
                        .and_then(positive_decimal)
                })
                .collect();
            if class_ids.len() != 1
                || card.value().attr("filename").is_some()
                || card.value().attr("oldline").is_some()
                || card.value().attr("newline").is_some()
            {
                return Err(invalid());
            }
            (class_ids[0], ReviewCommentKind::Commit)
        };
        if !ids.insert(storage_id) {
            return Err(invalid());
        }
        let line = |name: &str| -> Result<Option<u64>> {
            card.value()
                .attr(name)
                .map(|value| positive_decimal(value).ok_or_else(invalid))
                .transpose()
        };
        let body = one(card
            .select(&selector(&format!(".commit-commentContent-{storage_id}")))
            .filter(|node| !inside_body(*node)))?;
        let author = one(card
            .select(&selector("a.username[href]"))
            .filter(|node| !inside_body(*node)))?;
        let login = author.text().collect::<String>();
        let login = login.trim();
        url_safety::identifier(login, "review author").map_err(|_| invalid())?;
        let expected_author = url_safety::append(web_root, &[login]).map_err(|_| invalid())?;
        let actual_author = author
            .value()
            .attr("href")
            .and_then(|href| page_url.join(href).ok())
            .ok_or_else(invalid)?;
        if !same_target(&actual_author, &expected_author) {
            return Err(invalid());
        }
        let dates: Vec<_> = card
            .select(&selector("span[data-toggle=tooltip][title]"))
            .filter(|node| !inside_body(*node))
            .collect();
        if dates.len() > 1 {
            return Err(invalid());
        }
        let current_commit = if kind == ReviewCommentKind::Commit {
            let link = one(card
                .select(&selector("a.monospace[href]"))
                .filter(|node| !inside_body(*node)))?;
            let url = link
                .value()
                .attr("href")
                .and_then(|href| page_url.join(href).ok())
                .ok_or_else(invalid)?;
            let repo_path = page_url.path().rsplit_once("/pull/").ok_or_else(invalid)?.0;
            let prefix = format!("{repo_path}/commit/");
            let sha = url
                .path()
                .strip_prefix(&prefix)
                .filter(|sha| sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(invalid)?;
            if url.origin() != page_url.origin()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(invalid());
            }
            Some(sha.to_owned())
        } else {
            None
        };
        let mut url = page_url.clone();
        if kind != ReviewCommentKind::Commit {
            url.set_fragment(Some(&format!("discussion_r{storage_id}")));
        }
        comments.push(ReviewComment {
            id: format!("commit:{storage_id}"),
            storage_id,
            kind,
            body_raw: None,
            body_html: body.inner_html(),
            body_format: ReviewBodyFormat::Html,
            author: login.into(),
            created_at: None,
            created_at_display: dates
                .first()
                .and_then(|node| node.value().attr("title"))
                .map(str::to_owned),
            updated_at: None,
            path: card.value().attr("filename").map(str::to_owned),
            observed_old_line: line("oldline")?,
            observed_new_line: line("newline")?,
            current_commit,
            original_commit: None,
            original_old_line: None,
            original_new_line: None,
            position_verified: false,
            association: ReviewAssociation::ObservedOnPullRequestPage,
            source: ReviewSource::GitbucketWebConversation,
            url: url.into(),
        });
    }
    let bodies = list
        .select(&selector("[class*='commit-commentContent-']"))
        .filter(|node| !inside_body(*node))
        .count();
    if bodies != comments.len() {
        return Err(invalid());
    }
    let ordinary = list
        .select(&selector(".issue-comment-box[id^='comment-']"))
        .filter(|node| !inside_body(*node))
        .count();
    let positioned = comments
        .iter()
        .filter(|c| c.kind != ReviewCommentKind::Commit)
        .count();
    if badge > positioned + ordinary {
        return Err(invalid());
    }
    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn url() -> Url {
        Url::parse("http://gitbucket.test/probe-owner/probe-repo/pull/1").unwrap()
    }
    fn page(cards: &str, badge: usize) -> String {
        format!("<span id='show-title'>PR</span><ul id='pullreq-tab'><li class='active'><a href='/probe-owner/probe-repo/pull/1'>Conversation<span class='badge'>{badge}</span></a></li><li><a href='/probe-owner/probe-repo/pull/1/commits'>Commits</a></li><li><a href='/probe-owner/probe-repo/pull/1/files'>Files</a></li></ul><div id='comment-list'><div class='issue-comment-box'><div id='issueContent'>PR body</div></div>{cards}</div>")
    }
    fn read(html: &str) -> Result<Vec<ReviewComment>> {
        parse(html, &url(), "http://gitbucket.test")
    }
    fn cards() -> String {
        include_str!("../../tests/fixtures/inline_comments/4.48.0/conversation_before.html")
            .replace("/commit/FIRST", &format!("/commit/{}", "a".repeat(40)))
    }

    #[test]
    fn both_official_fixtures_retain_all_kinds_ids_positions_and_html() {
        for fixture in [
            include_str!("../../tests/fixtures/inline_comments/4.44.0/conversation_before.html"),
            include_str!("../../tests/fixtures/inline_comments/4.48.0/conversation_before.html"),
        ] {
            let fixture = fixture.replace("/commit/FIRST", &format!("/commit/{}", "a".repeat(40)));
            let comments = read(&page(&fixture, 10)).unwrap();
            assert_eq!(comments.len(), 11);
            assert_eq!(
                comments
                    .iter()
                    .map(|c| c.storage_id)
                    .collect::<HashSet<_>>(),
                (1..=11).collect()
            );
            assert_eq!(comments[0].kind, ReviewCommentKind::Commit);
            assert_eq!(
                comments[0].current_commit.as_deref(),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            );
            assert_eq!(comments[1].kind, ReviewCommentKind::File);
            let line = comments.iter().find(|c| c.storage_id == 3).unwrap();
            assert!(line.body_html.contains("<strong>bold</strong>"));
            assert_eq!(line.observed_new_line, Some(2));
            assert_eq!(
                comments
                    .iter()
                    .find(|c| c.storage_id == 5)
                    .unwrap()
                    .observed_old_line,
                Some(2)
            );
            assert_eq!(
                comments
                    .iter()
                    .find(|c| c.storage_id == 7)
                    .unwrap()
                    .path
                    .as_deref(),
                Some("renamed_日本語.txt")
            );
            assert_eq!(
                comments
                    .iter()
                    .find(|c| c.storage_id == 8)
                    .unwrap()
                    .path
                    .as_deref(),
                Some("quoted & name.txt")
            );
            for comment in comments {
                assert_eq!(comment.id, format!("commit:{}", comment.storage_id));
                assert!(
                    comment.body_raw.is_none()
                        && comment.created_at.is_none()
                        && comment.updated_at.is_none()
                );
                assert!(
                    comment.original_commit.is_none()
                        && comment.original_old_line.is_none()
                        && comment.original_new_line.is_none()
                );
                assert!(!comment.position_verified);
                assert_eq!(
                    comment.author,
                    if comment.storage_id == 10 {
                        "probe-reader"
                    } else {
                        "probe-owner"
                    }
                );
                if comment.kind != ReviewCommentKind::Commit {
                    assert!(comment.current_commit.is_none());
                }
            }
        }
    }

    #[test]
    fn known_empty_and_ordinary_only_pages_are_successful_observations() {
        assert!(read(&page("", 0)).unwrap().is_empty());
        let ordinary = "<div class='issue-comment-box' id='comment-1'><div id='commentContent-1'>normal</div></div>";
        assert!(read(&page(ordinary, 1)).unwrap().is_empty());
    }

    #[test]
    fn folded_cards_same_line_comments_and_unverified_relocations_are_retained() {
        let fixture = include_str!("../../tests/fixtures/inline_comments/4.48.0/new_line.html");
        let folded = format!("<div class='panel'><div class='fold-comments-3' style='display: none'>{fixture}</div></div>");
        assert_eq!(read(&page(&folded, 1)).unwrap()[0].storage_id, 3);
        let moved = fixture.replace("newline=\"2\"", "oldline=\"7\"");
        let result = read(&page(&moved, 1)).unwrap();
        assert_eq!(result[0].observed_old_line, Some(7));
        assert_eq!(result[0].observed_new_line, None);
        assert_eq!(result[0].original_old_line, None);
        let comments = read(&page(&cards(), 10)).unwrap();
        let ids: Vec<_> = comments
            .iter()
            .filter(|c| c.path.as_deref() == Some("review.txt") && c.observed_new_line == Some(2))
            .map(|c| c.storage_id)
            .collect();
        assert_eq!(ids, [3, 4, 9]);
    }

    #[test]
    fn unknown_structure_duplicates_foreign_metadata_and_malformed_positions_fail_closed() {
        let good = page(&cards(), 10);
        for bad in [
            "<html>Not a PR</html>".into(),
            good.replace("id='comment-list'", "id='changed-list'"),
            good.replace("id='pullreq-tab'", "id='changed-tabs'"),
            good.replace(
                "href='/probe-owner/probe-repo/pull/1'",
                "href='/other/repo/pull/1'",
            ),
            good.replace("discussion_r3", "discussion_rx"),
            good.replace("newline=\"2\"", "newline=\"0\""),
            good.replace("commit-commentContent-3", "unknown-body-3"),
            good.replace(
                "href=\"/probe-owner\" class=\"username",
                "href=\"https://foreign.test/probe-owner\" class=\"username",
            ),
            good.replace("/probe-owner/probe-repo/commit/", "/other/repo/commit/"),
            page("<article class='new-review-card'>lost</article>", 0),
            page("", 1),
            page(
                &(cards()
                    + include_str!("../../tests/fixtures/inline_comments/4.48.0/new_line.html")),
                11,
            ),
        ] {
            let error = read(&bad).unwrap_err();
            assert!(!error.to_string().contains("foreign.test"));
        }
    }

    #[test]
    fn metadata_like_user_markup_is_returned_only_as_body_data() {
        let fixture = include_str!("../../tests/fixtures/inline_comments/4.48.0/new_line.html");
        let fake = "<div id='issueContent'>body-like ID</div><div id='pullreq-tab'>nav-like ID</div><span id='show-title'>title-like ID</span><div class='commit-comment-box' id='discussion_r99' filename='fake'><a class='username' href='https://foreign.test/evil'>evil</a><div class='commit-commentContent-99'>data</div></div>";
        let fixture = fixture.replace("<p>indented code</p>", fake);
        let comments = read(&page(&fixture, 1)).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].storage_id, 3);
        assert_eq!(comments[0].author, "probe-owner");
        assert!(comments[0].body_html.contains("discussion_r99"));
    }
}
