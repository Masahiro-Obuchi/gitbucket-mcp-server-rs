//! HTML interpretation for the verified GitBucket web contract.
use crate::error::{GbMcpError, Result};
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static HTML selector")
}

pub(super) fn check_page(body: &str) -> Result<()> {
    let html = Html::parse_document(body);
    if html.select(&selector("input[type=password], form[action*='/signin'], form[action*='/saml'], form[action*='/oauth'], form[action*='/oidc']")).next().is_some() {
        return Err(GbMcpError::Authentication("GitBucket returned an authentication form instead of the requested response"));
    }
    if html
        .select(&selector(
            "[id^='error-'], .alert-danger, .has-error .help-block",
        ))
        .any(|e| e.text().any(|t| !t.trim().is_empty()))
    {
        return Err(GbMcpError::Other(
            "GitBucket returned a web validation/error page; no success was verified".into(),
        ));
    }
    Ok(())
}

/// Parse actual row boundaries and decoded query values, never nearby text.
pub(super) fn label_id(body: &str, issues_url: &Url, name: &str) -> Result<u64> {
    check_page(body)?;
    let mut ids = HashSet::new();
    // HTML fragments containing <tr> need a table context in the HTML5 parser.
    for html in [
        Html::parse_document(body),
        Html::parse_fragment(&format!("<table><tbody>{body}</tbody></table>")),
    ] {
        for row in html.select(&selector("tr[id^='label-row-']")) {
            let Some(id) = row
                .value()
                .attr("id")
                .and_then(|id| id.strip_prefix("label-row-"))
                .filter(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
                .and_then(|id| id.parse::<u64>().ok())
            else {
                continue;
            };
            for link in row.select(&selector("a[href]")) {
                let Some(url) = link
                    .value()
                    .attr("href")
                    .and_then(|href| issues_url.join(href).ok())
                else {
                    continue;
                };
                if url.origin() != issues_url.origin()
                    || url.path() != issues_url.path()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    continue;
                }
                let labels: Vec<_> = url
                    .query_pairs()
                    .filter(|(key, _)| key == "labels")
                    .map(|(_, value)| value.into_owned())
                    .collect();
                if labels == [name] {
                    ids.insert(id);
                }
            }
        }
    }
    if ids.len() != 1 {
        return Err(GbMcpError::Other(
            "Could not uniquely identify the exact label row in GitBucket HTML".into(),
        ));
    }
    Ok(*ids.iter().next().expect("one matching row"))
}

pub(super) fn milestone_list(body: &str, new_url: &Url) -> Result<()> {
    check_page(body)?;
    let html = Html::parse_document(body);
    if html.select(&selector("a[href]")).any(|e| {
        e.value()
            .attr("href")
            .and_then(|href| new_url.join(href).ok())
            .is_some_and(|url| url.origin() == new_url.origin() && url.path() == new_url.path())
    }) {
        Ok(())
    } else {
        Err(GbMcpError::Other(
            "GitBucket did not return the expected milestone list page".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn issues() -> Url {
        Url::parse("https://example.test/team/owner/repo/issues").unwrap()
    }
    #[test]
    fn label_lookup_decodes_html_entities_and_query_encoding_exactly() {
        let name = "日本語 & \"quoted\" + %";
        let mut target = issues();
        target
            .query_pairs_mut()
            .append_pair("state", "all")
            .append_pair("labels", name);
        let href = target.as_str().replace('&', "&amp;");
        let html=format!("<table><tbody><tr id='label-row-52'><td><a href='{href}'>{name}</a></td></tr></tbody></table>");
        assert_eq!(label_id(&html, &issues(), name).unwrap(), 52);
        assert!(label_id(&html, &issues(), "日本語").is_err());
    }
    #[test]
    fn label_rows_are_structural_and_ambiguous_or_foreign_links_are_rejected() {
        for html in [
            r#"label-row-1 <a href='/team/owner/repo/issues?labels=bug'>bug</a>"#,
            r#"<tr id='label-row-1'><a href='/team/owner/repo/issues?labels=bugfix'>bugfix</a></tr>"#,
            r#"<tr id='label-row-1'><a href='https://elsewhere.test/team/owner/repo/issues?labels=bug'>bug</a></tr>"#,
            r#"<tr id='label-row-1'><a href='/team/other/repo/issues?labels=bug'>bug</a></tr>"#,
            r#"<tr id='label-row-1x'><a href='/team/owner/repo/issues?labels=bug'>bug</a></tr>"#,
            r#"<tr id='label-row-1'><a href='/team/owner/repo/issues?labels=bug&amp;labels=other'>bug</a></tr>"#,
            r#"<tr id='label-row-1'><a href='/team/owner/repo/issues?labels=bug'>bug</a></tr><tr id='label-row-2'><a href='/team/owner/repo/issues?labels=bug'>bug</a></tr>"#,
        ] {
            assert!(label_id(html, &issues(), "bug").is_err(), "{html}");
        }
        let fragment = r#"<tr id='label-row-7'><td><a href='/team/owner/repo/issues?labels=bug'>bug</a></td></tr>"#;
        assert_eq!(label_id(fragment, &issues(), "bug").unwrap(), 7);
    }
    #[test]
    fn auth_and_validation_forms_are_not_success_and_errors_do_not_echo_contents() {
        for html in [
            r#"<form action='/signin'><input type='password' value='fake-secret'></form>"#,
            r#"<form action='/signin/oidc'><button>SSO</button></form>"#,
            r#"<span id='error-title' class='error'>fake-secret</span>"#,
        ] {
            let error = check_page(html).unwrap_err();
            assert!(!format!("{error:?}").contains("fake-secret"));
        }
        assert!(check_page("<span id='error-title' class='error'></span>").is_ok());
    }
}
