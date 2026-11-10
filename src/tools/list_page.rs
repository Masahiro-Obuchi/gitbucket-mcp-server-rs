//! Bounded, server-issued continuations over a collected observation, not an atomic source snapshot.
use super::{
    list_summary,
    response::{from_gb_error, success, ToolResult},
};
use crate::{
    error::{GbMcpError, Result},
    models::{
        list_page::{ListOptions, ListView},
        review_comment::ReviewCoverage,
    },
    server::GitBucketMcpServer,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    collections::HashSet,
    future::Future,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PAGE_BYTES: usize = 64 * 1024;
const OBSERVATION_BYTES: usize = 2 * 1024 * 1024;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_ENTRIES: usize = 16;
const MAX_ITEMS: usize = 10_000;
const TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListKind {
    Repositories,
    Branches,
    Labels,
    Milestones,
    Issues,
    PullRequests,
    Comments,
    ReviewComments,
    PullRequestCommits,
    PullRequestFiles,
    PullRequestDiff,
}
impl ListKind {
    fn field(self) -> &'static str {
        match self {
            Self::Repositories => "repositories",
            Self::Branches => "branches",
            Self::Labels => "labels",
            Self::Milestones => "milestones",
            Self::Issues => "issues",
            Self::PullRequests => "pull_requests",
            Self::Comments | Self::ReviewComments => "comments",
            Self::PullRequestCommits => "commits",
            Self::PullRequestFiles => "files",
            Self::PullRequestDiff => "chunks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListScope {
    kind: ListKind,
    owner: String,
    repo: Option<String>,
    number: Option<u64>,
    state: String,
    path: Option<String>,
}
impl ListScope {
    pub(crate) fn new(kind: ListKind, owner: &str, repo: Option<&str>) -> Self {
        Self {
            kind,
            owner: owner.into(),
            repo: repo.map(str::to_owned),
            number: None,
            state: "open".into(),
            path: None,
        }
    }
    pub(crate) fn numbered(mut self, number: u64) -> Self {
        self.number = Some(number);
        self
    }
    pub(crate) fn path(mut self, path: Option<&str>) -> Self {
        self.path = path.map(str::to_owned);
        self
    }
    pub(crate) fn state(mut self, state: Option<&str>) -> Self {
        self.state = state.unwrap_or("open").into();
        self
    }
}

pub(crate) struct ListObservation {
    items: Vec<Value>,
    extra: Map<String, Value>,
    source_complete: bool,
}
impl ListObservation {
    pub(crate) fn with_metadata<T: Serialize>(mut self, key: &str, value: &T) -> Result<Self> {
        self.extra.insert(key.into(), serde_json::to_value(value)?);
        Ok(self)
    }
    pub(crate) fn new<T: Serialize>(
        items: Vec<T>,
        coverage: Option<ReviewCoverage>,
    ) -> Result<Self> {
        let source_complete = coverage.as_ref().is_none_or(|c| c.complete);
        let mut extra = Map::new();
        if let Some(coverage) = coverage {
            extra.insert("coverage".into(), serde_json::to_value(coverage)?);
        }
        Ok(Self {
            items: items
                .into_iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<_, _>>()?,
            extra,
            source_complete,
        })
    }
}

struct Observation {
    id: String,
    scope: ListScope,
    view: ListView,
    limit: usize,
    items: Vec<Value>,
    extra: Map<String, Value>,
    source_complete: bool,
    bytes: usize,
    observed_at: u128,
    created: Instant,
}
struct Entry {
    observation: Arc<Observation>,
    issued: HashSet<usize>,
}
#[derive(Default)]
pub(crate) struct ListCache {
    entries: Vec<Entry>,
}
impl std::fmt::Debug for ListCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListCache")
            .field("observations", &self.entries.len())
            .finish_non_exhaustive()
    }
}
fn bad_cursor() -> GbMcpError {
    GbMcpError::InvalidInput("Unknown, expired or mismatched list cursor; restart without a cursor using the same desired filters/view/limit".into())
}
fn token(id: &str, offset: usize) -> String {
    format!("{id}.{offset}")
}
fn response_bytes(value: &Value) -> Result<usize> {
    // rmcp includes both structuredContent and its text fallback. Bound the
    // complete CallToolResult, including that duplication and JSON escaping.
    Ok(serde_json::to_vec(&rmcp::model::CallToolResult::structured(value.clone()))?.len())
}
fn parse_cursor(cursor: &str) -> Result<(&str, usize)> {
    if cursor.len() > 64 {
        return Err(bad_cursor());
    }
    let (id, offset) = cursor.split_once('.').ok_or_else(bad_cursor)?;
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || offset.starts_with('0')
        || !offset.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(bad_cursor());
    }
    let offset = offset
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(bad_cursor)?;
    Ok((id, offset))
}
impl ListCache {
    fn prune(&mut self) {
        self.entries
            .retain(|entry| entry.observation.created.elapsed() < TTL);
    }
    fn continue_at(
        &mut self,
        cursor: &str,
        scope: &ListScope,
        limit: usize,
        view: ListView,
    ) -> Result<(Arc<Observation>, usize)> {
        self.prune();
        let (id, offset) = parse_cursor(cursor)?;
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.observation.id == id)
            .ok_or_else(bad_cursor)?;
        let o = &entry.observation;
        if o.scope != *scope
            || o.limit != limit
            || o.view != view
            || !entry.issued.contains(&offset)
        {
            return Err(bad_cursor());
        }
        Ok((o.clone(), offset))
    }
    fn insert(&mut self, observation: Arc<Observation>, next: usize) {
        self.prune();
        while self.entries.len() >= CACHE_ENTRIES
            || self
                .entries
                .iter()
                .map(|e| e.observation.bytes)
                .sum::<usize>()
                + observation.bytes
                > CACHE_BYTES
        {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            observation,
            issued: HashSet::from([next]),
        });
    }
}

impl Observation {
    fn render(&self, start: usize, end: usize) -> Value {
        let more = end < self.items.len();
        let mut object = self.extra.clone();
        object.insert(
            self.scope.kind.field().into(),
            json!(&self.items[start..end]),
        );
        object.insert(
            "next_cursor".into(),
            if more {
                json!(token(&self.id, end))
            } else {
                Value::Null
            },
        );
        object.insert("has_more".into(), json!(more));
        object.insert("range".into(),json!({
            "start":start,"end":end,"returned":end-start,"observed_total":self.items.len(),"limit":self.limit,
            "view":self.view,"order":"upstream_observed","consistency":"cached_observation","atomic":false,
            "source_complete":self.source_complete,"observation_id":self.id,
            "observed_at_unix_ms":self.observed_at,"expires_at_unix_ms":self.observed_at+TTL.as_millis(),
            "response_byte_limit":PAGE_BYTES,
            "limited_by":if !more {None}else if end-start<self.limit {Some("bytes")}else{Some("count")},
        }));
        Value::Object(object)
    }
    fn page(&self, start: usize) -> Result<(Value, usize)> {
        let stop = (start + self.limit).min(self.items.len());
        let mut end = start;
        let mut value = self.render(start, end);
        while end < stop {
            let candidate = self.render(start, end + 1);
            if response_bytes(&candidate)? > PAGE_BYTES {
                break;
            }
            value = candidate;
            end += 1;
        }
        if (end == start && start < self.items.len()) || response_bytes(&value)? > PAGE_BYTES {
            return Err(GbMcpError::ResponseTooLarge {
                kind: "MCP list page (try summary view or a detail getter)",
                limit: PAGE_BYTES,
            });
        }
        Ok((value, end))
    }
}

impl GitBucketMcpServer {
    pub(crate) async fn list_response(
        &self,
        scope: ListScope,
        options: ListOptions,
        fetch: impl Future<Output = Result<ListObservation>>,
    ) -> ToolResult {
        match self.list_value(scope, options, fetch).await {
            Ok(value) => success(&value),
            Err(error) => from_gb_error(error),
        }
    }
    async fn list_value(
        &self,
        scope: ListScope,
        options: ListOptions,
        fetch: impl Future<Output = Result<ListObservation>>,
    ) -> Result<Value> {
        let limit = options.limit.unwrap_or(50);
        if !(1..=100).contains(&limit) {
            return Err(GbMcpError::InvalidInput(
                "limit must be between 1 and 100".into(),
            ));
        }
        let limit = limit as usize;
        let view = options.view.unwrap_or_default();
        if let Some(cursor) = options.cursor {
            let (observation, start) = self
                .lists
                .lock()
                .expect("list cache")
                .continue_at(&cursor, &scope, limit, view)?;
            let (page, next) = observation.page(start)?;
            if next < observation.items.len() {
                let mut cache = self.lists.lock().expect("list cache");
                if let Some(entry) = cache
                    .entries
                    .iter_mut()
                    .find(|e| e.observation.id == observation.id)
                {
                    entry.issued.insert(next);
                } else {
                    return Err(bad_cursor());
                }
            }
            return Ok(page);
        }
        let data = fetch.await?;
        if data.items.len() > MAX_ITEMS {
            return Err(GbMcpError::Unsupported(format!(
                "MCP list observation exceeds the {MAX_ITEMS}-item limit; refine the query"
            )));
        }
        let mut items = Vec::with_capacity(data.items.len());
        let mut bytes = serde_json::to_vec(&data.extra)?.len();
        for item in data.items {
            let item = if view == ListView::Summary {
                list_summary::project(scope.kind, item)
            } else {
                item
            };
            bytes += serde_json::to_vec(&item)?.len();
            if bytes > OBSERVATION_BYTES {
                return Err(GbMcpError::ResponseTooLarge {
                    kind: "MCP list observation (try summary view)",
                    limit: OBSERVATION_BYTES,
                });
            }
            items.push(item);
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| GbMcpError::Other("Could not issue a list observation ID".into()))?;
        let id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let observation = Arc::new(Observation {
            id,
            scope,
            view,
            limit,
            items,
            extra: data.extra,
            source_complete: data.source_complete,
            bytes,
            observed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            created: Instant::now(),
        });
        // Do not issue a cursor that inevitably stops at an unpageable item.
        for index in 0..observation.items.len() {
            if response_bytes(&observation.render(index, index + 1))? > PAGE_BYTES {
                return Err(GbMcpError::ResponseTooLarge {
                    kind: "MCP list item (try summary view or a detail getter)",
                    limit: PAGE_BYTES,
                });
            }
        }
        let (page, next) = observation.page(0)?;
        if next < observation.items.len() {
            self.lists
                .lock()
                .expect("list cache")
                .insert(observation, next);
        }
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(id: u64, bytes: usize, created: Instant) -> Arc<Observation> {
        Arc::new(Observation {
            id: format!("{id:032x}"),
            scope: ListScope::new(ListKind::Branches, "owner", Some("repo")),
            view: ListView::Summary,
            limit: 1,
            items: vec![json!({"name":"a"}), json!({"name":"b"})],
            extra: Map::new(),
            source_complete: true,
            bytes,
            observed_at: 0,
            created,
        })
    }

    #[test]
    fn count_and_byte_pressure_evict_oldest_observations_and_expiry_rejects_without_refresh() {
        let scope = ListScope::new(ListKind::Branches, "owner", Some("repo"));
        let mut cache = ListCache::default();
        for id in 1..=17 {
            cache.insert(observation(id, 100, Instant::now()), 1);
        }
        assert_eq!(cache.entries.len(), 16);
        assert!(cache
            .continue_at(
                &token(&format!("{:032x}", 1), 1),
                &scope,
                1,
                ListView::Summary
            )
            .is_err());
        assert!(cache
            .continue_at(
                &token(&format!("{:032x}", 17), 1),
                &scope,
                1,
                ListView::Summary
            )
            .is_ok());
        cache = ListCache::default();
        for id in 1..=5 {
            cache.insert(observation(id, OBSERVATION_BYTES, Instant::now()), 1);
        }
        assert_eq!(cache.entries.len(), 4);
        assert!(cache
            .continue_at(
                &token(&format!("{:032x}", 1), 1),
                &scope,
                1,
                ListView::Summary
            )
            .is_err());
        let expired = observation(6, 100, Instant::now() - TTL);
        cache.entries.push(Entry {
            observation: expired,
            issued: HashSet::from([1]),
        });
        assert!(cache
            .continue_at(
                &token(&format!("{:032x}", 6), 1),
                &scope,
                1,
                ListView::Summary
            )
            .is_err());
    }

    #[tokio::test]
    async fn observation_limits_return_errors_without_issuing_a_partial_cursor() {
        let server = GitBucketMcpServer::new(
            crate::api::client::GitBucketClient::new("https://example.invalid", "fake").unwrap(),
        );
        let scope = ListScope::new(ListKind::Comments, "owner", Some("repo")).numbered(1);
        let items: Vec<_> = (0..MAX_ITEMS + 1).map(|id| json!({"id":id})).collect();
        assert!(matches!(
            server
                .list_value(scope.clone(), ListOptions::default(), async {
                    ListObservation::new(items, None)
                })
                .await,
            Err(GbMcpError::Unsupported(_))
        ));
        let items = vec![json!({"id":1,"body":"a".repeat(OBSERVATION_BYTES+1)})];
        assert!(matches!(
            server
                .list_value(
                    scope.clone(),
                    ListOptions {
                        view: Some(ListView::Full),
                        ..Default::default()
                    },
                    async { ListObservation::new(items, None) }
                )
                .await,
            Err(GbMcpError::ResponseTooLarge { .. })
        ));
        // A large item later in the traversal must fail before any page is issued.
        let items = vec![
            json!({"id":1,"body":"small"}),
            json!({"id":2,"body":"b".repeat(PAGE_BYTES)}),
        ];
        assert!(matches!(
            server
                .list_value(
                    scope,
                    ListOptions {
                        limit: Some(1),
                        view: Some(ListView::Full),
                        ..Default::default()
                    },
                    async { ListObservation::new(items, None) }
                )
                .await,
            Err(GbMcpError::ResponseTooLarge { .. })
        ));
        assert!(server.lists.lock().unwrap().entries.is_empty());
    }
}
