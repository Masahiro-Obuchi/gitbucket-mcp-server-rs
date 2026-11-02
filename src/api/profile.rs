//! Upstream operation contracts; profiles do not infer capabilities from failed writes.
use crate::error::{GbMcpError, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ApiProfile {
    #[default]
    RestOnly,
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    IssueUpdate,
    PullRequestUpdate,
    LabelRename,
    LabelSameNameUpdate,
    MilestoneWrite,
    MilestoneRestrictedTitle,
    MilestoneDelete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Rest,
    Web,
}

#[derive(Debug, Clone, Copy)]
pub struct Capability {
    pub route: Route,
    pub evidence: &'static str,
}

impl ApiProfile {
    pub fn from_env() -> Result<Self> {
        match std::env::var("GITBUCKET_API_PROFILE") {
            Ok(value) => value.parse(),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(_) => Err(invalid_mode()),
        }
    }

    pub fn capability(self, operation: Operation) -> Capability {
        let evidence = match self {
            Self::RestOnly => "Unverified instance; REST only, no web writes",
            Self::Standard => "Standard GitBucket contract verified on 4.44.0 and 4.48.0",
        };
        let route = match self {
            Self::RestOnly => Route::Rest,
            Self::Standard => match operation {
                Operation::IssueUpdate
                | Operation::LabelSameNameUpdate
                | Operation::MilestoneRestrictedTitle => Route::Web,
                _ => Route::Rest,
            },
        };
        Capability { route, evidence }
    }
}

impl std::str::FromStr for ApiProfile {
    type Err = GbMcpError;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "rest-only" => Ok(Self::RestOnly),
            "standard" => Ok(Self::Standard),
            _ => Err(invalid_mode()),
        }
    }
}
fn invalid_mode() -> GbMcpError {
    GbMcpError::Config("GITBUCKET_API_PROFILE must be rest-only or standard".into())
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
        std::env::remove_var("GITBUCKET_API_PROFILE");
        assert_eq!(ApiProfile::from_env().unwrap(), ApiProfile::RestOnly);
        std::env::set_var("GITBUCKET_API_PROFILE", "standard");
        assert_eq!(ApiProfile::from_env().unwrap(), ApiProfile::Standard);
        std::env::set_var("GITBUCKET_API_PROFILE", "fake-secret-mode");
        assert!(!ApiProfile::from_env()
            .unwrap_err()
            .to_string()
            .contains("fake-secret-mode"));
        std::env::remove_var("GITBUCKET_API_PROFILE");
    }
}
