//! Shared transport budgets; REST authorization and web cookies stay separate.
use crate::error::{GbMcpError, Result};
use reqwest::{ClientBuilder, Response};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct HttpSettings {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub merge_timeout: Duration,
    pub tool_timeout: Duration,
    pub json_limit_bytes: usize,
    pub html_limit_bytes: usize,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            merge_timeout: Duration::from_secs(120),
            tool_timeout: Duration::from_secs(180),
            json_limit_bytes: 8 * 1024 * 1024,
            html_limit_bytes: 4 * 1024 * 1024,
        }
    }
}

impl HttpSettings {
    /// Runtime overrides are env-only. Never include supplied values in diagnostics.
    pub fn from_env() -> Result<Self> {
        let mut settings = Self::default();
        for (key, slot) in [
            (
                "GITBUCKET_CONNECT_TIMEOUT_SECS",
                &mut settings.connect_timeout,
            ),
            (
                "GITBUCKET_REQUEST_TIMEOUT_SECS",
                &mut settings.request_timeout,
            ),
            ("GITBUCKET_MERGE_TIMEOUT_SECS", &mut settings.merge_timeout),
            ("GITBUCKET_TOOL_TIMEOUT_SECS", &mut settings.tool_timeout),
        ] {
            if let Some(value) = env_number(key)? {
                *slot = Duration::from_secs(value);
            }
        }
        for (key, slot) in [
            ("GITBUCKET_JSON_LIMIT_BYTES", &mut settings.json_limit_bytes),
            ("GITBUCKET_HTML_LIMIT_BYTES", &mut settings.html_limit_bytes),
        ] {
            if let Some(value) = env_number(key)? {
                *slot = usize::try_from(value)
                    .map_err(|_| GbMcpError::Config(format!("{key} is out of range")))?;
            }
        }
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> Result<()> {
        for duration in [
            self.connect_timeout,
            self.request_timeout,
            self.merge_timeout,
            self.tool_timeout,
        ] {
            if duration.is_zero() || duration > Duration::from_secs(86400) {
                return Err(GbMcpError::Config(
                    "HTTP/tool timeouts must be positive and at most 86400 seconds".into(),
                ));
            }
        }
        for limit in [self.json_limit_bytes, self.html_limit_bytes] {
            if limit == 0 || limit > 256 * 1024 * 1024 {
                return Err(GbMcpError::Config(
                    "Response limits must be between 1 and 268435456 bytes".into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn configure(&self, builder: ClientBuilder) -> ClientBuilder {
        builder
            .connect_timeout(self.connect_timeout)
            .timeout(self.request_timeout)
            // No transparent retry, including HTTP/2 protocol-level retries.
            .retry(reqwest::retry::never())
    }
}

fn env_number(key: &str) -> Result<Option<u64>> {
    match std::env::var(key) {
        Ok(value) => value
            .parse()
            .map(Some)
            .map_err(|_| GbMcpError::Config(format!("{key} must be a positive integer"))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(GbMcpError::Config(format!(
            "{key} must be a positive integer"
        ))),
    }
}

/// Limit while streaming, including when Content-Length is absent or misleading.
pub(crate) async fn bounded_text(
    mut response: Response,
    limit: usize,
    kind: &'static str,
) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(GbMcpError::ResponseTooLarge { kind, limit });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(GbMcpError::from)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(GbMcpError::ResponseTooLarge { kind, limit });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Per-call state also supports overlapping writes within one adapter operation.
pub(crate) struct WriteTracker {
    possible: AtomicBool,
    in_flight: AtomicUsize,
}
impl WriteTracker {
    pub(crate) fn new(untracked_write: bool) -> Self {
        Self {
            possible: AtomicBool::new(untracked_write),
            in_flight: AtomicUsize::new(0),
        }
    }
}
tokio::task_local! { pub(crate) static WRITE_POSSIBLE: Arc<WriteTracker>; }

pub(crate) fn write_possible() -> bool {
    WRITE_POSSIBLE
        .try_with(|state| {
            state.possible.load(Ordering::Relaxed) || state.in_flight.load(Ordering::Relaxed) != 0
        })
        .unwrap_or(false)
}

struct MutationGuard {
    tracker: Option<Arc<WriteTracker>>,
    rejected: bool,
}
impl Drop for MutationGuard {
    fn drop(&mut self) {
        if let Some(state) = &self.tracker {
            // Cancellation drops the guard before a definite rejection is known.
            if !self.rejected {
                state.possible.store(true, Ordering::Relaxed);
            }
            state.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Wrap a business write in the calling task; never spawn it independently.
/// Used by custom GitBucketApi adapters that opt into tracks_write_attempts.
/// Dropping a pending mutation conservatively records a possibly applied write.
pub async fn mutation<T>(
    operation: &'static str,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    let tracker = WRITE_POSSIBLE.try_with(Arc::clone).ok();
    if let Some(state) = &tracker {
        state.in_flight.fetch_add(1, Ordering::Relaxed);
    }
    let mut guard = MutationGuard {
        tracker,
        rejected: false,
    };
    let result = future.await;
    let rejected = matches!(&result, Err(GbMcpError::Api {status:400..=499, ..}) if !matches!(&result, Err(GbMcpError::Api {status:408, ..})));
    guard.rejected = rejected;
    if rejected {
        result
    } else {
        result.map_err(|error| unknown(operation, error))
    }
}

pub(crate) fn unknown(operation: &'static str, error: GbMcpError) -> GbMcpError {
    match error {
        err @ (GbMcpError::OperationOutcomeUnknown { .. }
        | GbMcpError::WriteOutcomeUnknown { .. }) => err,
        cause => GbMcpError::OperationOutcomeUnknown {
            operation,
            cause: Box::new(cause),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_reject_unbounded_or_zero_limits() {
        let mut settings = HttpSettings::default();
        assert!(settings.validate().is_ok());
        settings.request_timeout = Duration::ZERO;
        assert!(settings.validate().is_err());
        settings.request_timeout = Duration::from_secs(u64::MAX);
        assert!(settings.validate().is_err());
        settings = HttpSettings::default();
        settings.json_limit_bytes = 0;
        assert!(settings.validate().is_err());
        settings.html_limit_bytes = usize::MAX;
        assert!(settings.validate().is_err());
    }

    #[test]
    #[serial_test::serial]
    fn env_overrides_are_validated_without_echoing_input() {
        let keys = [
            "GITBUCKET_CONNECT_TIMEOUT_SECS",
            "GITBUCKET_REQUEST_TIMEOUT_SECS",
            "GITBUCKET_MERGE_TIMEOUT_SECS",
            "GITBUCKET_TOOL_TIMEOUT_SECS",
            "GITBUCKET_JSON_LIMIT_BYTES",
            "GITBUCKET_HTML_LIMIT_BYTES",
        ];
        for (key, value) in keys.iter().zip(["1", "2", "3", "4", "500", "600"]) {
            std::env::set_var(key, value);
        }
        let settings = HttpSettings::from_env().unwrap();
        assert_eq!(settings.connect_timeout, Duration::from_secs(1));
        assert_eq!(settings.request_timeout, Duration::from_secs(2));
        assert_eq!(settings.merge_timeout, Duration::from_secs(3));
        assert_eq!(settings.tool_timeout, Duration::from_secs(4));
        assert_eq!(settings.json_limit_bytes, 500);
        assert_eq!(settings.html_limit_bytes, 600);
        std::env::set_var(keys[0], "fake-secret-value");
        assert!(!HttpSettings::from_env()
            .unwrap_err()
            .to_string()
            .contains("fake-secret-value"));
        for key in keys {
            std::env::remove_var(key);
        }
    }
    #[tokio::test]
    async fn rejecting_one_concurrent_mutation_does_not_clear_another_in_flight() {
        WRITE_POSSIBLE
            .scope(Arc::new(WriteTracker::new(false)), async {
                let (started_tx, started_rx) = tokio::sync::oneshot::channel();
                let pending = mutation("pending", async {
                    let _ = started_tx.send(());
                    std::future::pending::<Result<()>>().await
                });
                let rejected = async {
                    started_rx.await.unwrap();
                    let error = mutation::<()>("rejected", async {
                        Err(GbMcpError::Api {
                            status: 403,
                            message: "Forbidden".into(),
                        })
                    })
                    .await
                    .unwrap_err();
                    assert!(matches!(error, GbMcpError::Api { status: 403, .. }));
                    assert!(write_possible());
                };
                tokio::select! { _ = pending => unreachable!(), _ = rejected => {} }
                // Dropping the pending write keeps its outcome unknown.
                assert!(write_possible());
            })
            .await;
    }
}
