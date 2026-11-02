//! Explicit operation contracts. A profile is an operator assertion, not version detection.
use crate::error::{GbMcpError, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compatibility {
    #[default]
    RestOnly,
    GitBucket444,
    GitBucket448,
    /// Explicit migration option for unverified older/custom instances.
    Legacy404,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    IssueUpdate,
    PullRequestUpdate,
    LabelRename,
    LabelSameNameUpdate,
    LabelDescription,
    MilestoneWrite,
    MilestoneRestrictedTitle,
    MilestoneDelete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Rest,
    Web,
    Unsupported,
    Unverified,
}

#[derive(Debug, Clone, Copy)]
pub struct Capability {
    pub route: Route,
    pub evidence: &'static str,
}

impl Compatibility {
    pub fn from_env() -> Result<Self> {
        match std::env::var("GITBUCKET_COMPATIBILITY") {
            Ok(value) => value.parse(),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(_) => Err(invalid_mode()),
        }
    }

    pub fn capability(self, operation: Operation) -> Capability {
        if operation == Operation::LabelDescription {
            return Capability {
                route: Route::Unsupported,
                evidence: "No supported label description write contract in this server",
            };
        }
        let evidence = match self {
            Self::RestOnly => "Unverified instance; REST only, no web writes",
            Self::Legacy404 => "Explicit operator opt-in to legacy HTTP 404 compatibility",
            Self::GitBucket444 => "Operator-selected standard GitBucket 4.44.0 source/E2E contract",
            Self::GitBucket448 => "Operator-selected standard GitBucket 4.48.0 source/E2E contract",
        };
        let route = match self {
            Self::RestOnly | Self::Legacy404 => Route::Unverified,
            Self::GitBucket444 | Self::GitBucket448 => match operation {
                Operation::IssueUpdate
                | Operation::LabelSameNameUpdate
                | Operation::MilestoneRestrictedTitle => Route::Web,
                _ => Route::Rest,
            },
        };
        Capability { route, evidence }
    }

    pub(crate) fn legacy_404(self) -> bool {
        self == Self::Legacy404
    }
}

impl std::str::FromStr for Compatibility {
    type Err = GbMcpError;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "rest-only" => Ok(Self::RestOnly),
            "gitbucket-4.44.0" => Ok(Self::GitBucket444),
            "gitbucket-4.48.0" => Ok(Self::GitBucket448),
            "legacy-404" => Ok(Self::Legacy404),
            _ => Err(invalid_mode()),
        }
    }
}
fn invalid_mode() -> GbMcpError {
    GbMcpError::Config("GITBUCKET_COMPATIBILITY must be rest-only, gitbucket-4.44.0, gitbucket-4.48.0 or legacy-404".into())
}

pub(crate) fn restricted_milestone_title(title: &str) -> bool {
    !title
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"-+_.".contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[serial_test::serial]
    fn env_mode_is_explicit_and_diagnostics_do_not_echo_values() {
        std::env::remove_var("GITBUCKET_COMPATIBILITY");
        assert_eq!(Compatibility::from_env().unwrap(), Compatibility::RestOnly);
        std::env::set_var("GITBUCKET_COMPATIBILITY", "gitbucket-4.48.0");
        assert_eq!(
            Compatibility::from_env().unwrap(),
            Compatibility::GitBucket448
        );
        std::env::set_var("GITBUCKET_COMPATIBILITY", "fake-secret-mode");
        assert!(!Compatibility::from_env()
            .unwrap_err()
            .to_string()
            .contains("fake-secret-mode"));
        std::env::remove_var("GITBUCKET_COMPATIBILITY");
    }
}
