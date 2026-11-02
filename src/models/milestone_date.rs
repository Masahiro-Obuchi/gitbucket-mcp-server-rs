use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use serde::{Serialize, Serializer};

use crate::error::{GbMcpError, Result};

/// GitBucket's JSON date format has whole-second precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MilestoneDate(DateTime<Utc>);

impl MilestoneDate {
    pub(crate) fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        let invalid = || {
            GbMcpError::InvalidInput(
                "due_on must be a valid YYYY-MM-DD date or RFC3339 timestamp".into(),
            )
        };
        let date = if input.len() == 10
            && input.is_ascii()
            && input.as_bytes()[4] == b'-'
            && input.as_bytes()[7] == b'-'
        {
            NaiveDate::parse_from_str(input, "%Y-%m-%d")
                .map_err(|_| invalid())?
                .and_hms_opt(0, 0, 0)
                .ok_or_else(invalid)?
                .and_utc()
        } else {
            DateTime::parse_from_rfc3339(input)
                .map_err(|_| invalid())?
                .with_timezone(&Utc)
        };
        if !(1..=9999).contains(&date.year()) || date.nanosecond() >= 1_000_000_000 {
            return Err(invalid());
        }
        Ok(Self(date.with_nanosecond(0).ok_or_else(invalid)?))
    }

    pub(crate) fn rfc3339(self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    /// A date-only web form cannot preserve a non-midnight instant.
    pub(crate) fn web_date(self) -> Result<String> {
        if self.0.hour() != 0 || self.0.minute() != 0 || self.0.second() != 0 {
            return Err(GbMcpError::Unsupported(
                "The GitBucket milestone web form cannot preserve a due_on time other than UTC midnight. Use a REST-capable operation or explicitly clear/set the date.".into(),
            ));
        }
        Ok(self.0.format("%Y-%m-%d").to_string())
    }
}

impl Serialize for MilestoneDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.rfc3339())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dates_normalize_to_utc_and_server_precision() {
        for (input, expected) in [
            ("2028-02-29", "2028-02-29T00:00:00Z"),
            ("2026-04-01T00:30:00+09:00", "2026-03-31T15:30:00Z"),
            ("2026-04-01T00:00:00.123Z", "2026-04-01T00:00:00Z"),
        ] {
            assert_eq!(MilestoneDate::parse(input).unwrap().rfc3339(), expected);
        }
    }

    #[test]
    fn test_invalid_dates_do_not_panic() {
        for value in [
            "2026-あいう",
            "2026-02-29",
            "2026-13-01",
            "2026-04-01Tinvalid",
            "",
            "0000-01-01",
            "2016-12-31T23:59:60Z",
        ] {
            assert!(MilestoneDate::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn test_web_dates_require_utc_midnight() {
        assert_eq!(
            MilestoneDate::parse("2026-04-01")
                .unwrap()
                .web_date()
                .unwrap(),
            "2026-04-01"
        );
        assert!(MilestoneDate::parse("2026-04-01T00:30:00Z")
            .unwrap()
            .web_date()
            .is_err());
    }
}
