//! URL boundaries shared by REST and web adapters.
use crate::error::{GbMcpError, Result};
use url::Url;

pub(crate) fn identifier(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.starts_with(['-', '_'])
        || !value.bytes().all(|c| {
            c.is_ascii_alphanumeric() || b"-_.".contains(&c) || (field == "repo" && c == b'+')
        })
    {
        return Err(GbMcpError::InvalidInput(format!("{field} contains invalid identifier characters or is a dot segment; use ASCII letters, digits, hyphens, underscores or dots (plus is also allowed for repo), without a leading hyphen or underscore")));
    }
    Ok(())
}

pub(crate) fn repository(owner: &str, repo: &str) -> Result<()> {
    identifier(owner, "owner")?;
    identifier(repo, "repo")
}

pub(crate) fn append(base: &str, segments: &[&str]) -> Result<Url> {
    if segments
        .iter()
        .any(|s| s.is_empty() || matches!(*s, "." | "..") || s.chars().any(char::is_control))
    {
        return Err(GbMcpError::InvalidInput(
            "URL segments must not be empty, dot segments or contain control characters".into(),
        ));
    }
    let mut url = Url::parse(base)?;
    url.path_segments_mut()
        .map_err(|_| GbMcpError::InvalidInput("URL cannot be a path base".into()))?
        .pop_if_empty()
        .extend(segments.iter().copied());
    Ok(url)
}

pub(crate) fn route(base: &str, path: &str) -> Result<Url> {
    let path = path
        .strip_prefix('/')
        .ok_or_else(|| GbMcpError::InvalidInput("API paths must start with /".into()))?;
    if path.contains(['?', '#', '%', '\\']) {
        return Err(GbMcpError::InvalidInput("API paths must not contain URL syntax or percent escapes; use query parameters or segment builders".into()));
    }
    append(base, &path.split('/').collect::<Vec<_>>())
}

pub(crate) fn validate_redirect(base: &Url, target: &Url) -> Result<()> {
    let prefix = base
        .path()
        .strip_suffix("/api/v3")
        .unwrap_or(base.path())
        .trim_end_matches('/');
    let path = target.path().to_ascii_lowercase();
    if target.origin() != base.origin()
        || !(target.path() == prefix || target.path().starts_with(&format!("{prefix}/")))
        || ["%2f", "%5c", "%2e", "%25"]
            .iter()
            .any(|escape| path.contains(escape))
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(GbMcpError::Other(
            "Redirect outside configured GitBucket origin or context path".into(),
        ));
    }
    Ok(())
}

pub(crate) fn redirect_policy(base: &str) -> Result<reqwest::redirect::Policy> {
    let base = Url::parse(base)?;
    Ok(reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("Too many redirects")
        } else if validate_redirect(&base, attempt.url()).is_err() {
            attempt.error("Redirect outside configured GitBucket origin or context path")
        } else {
            attempt.follow()
        }
    }))
}
