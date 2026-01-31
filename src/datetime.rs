//! Internal datetime abstraction supporting both chrono and jiff-datetime backends
//!
//! This module provides a unified API for datetime operations:
//! - `chrono` feature (default): Uses chrono crate
//! - `jiff-datetime` feature: Uses jiff crate
//!
//! These features are mutually exclusive.

use serde::{Deserialize, Serialize};
use std::fmt;

#[cfg(all(feature = "chrono", feature = "jiff-datetime"))]
compile_error!("Features 'chrono' and 'jiff-datetime' are mutually exclusive. Enable exactly one.");

#[cfg(not(any(feature = "chrono", feature = "jiff-datetime")))]
compile_error!(
    "Exactly one datetime backend must be enabled. Use 'chrono' (default) or 'jiff-datetime' feature."
);

#[cfg(feature = "chrono")]
mod imp {
    use super::*;
    pub use chrono::{DateTime as ChronoDateTime, Utc};

    /// UTC datetime type
    ///
    /// This type wraps either:
    /// - `chrono::DateTime<Utc>` when `chrono` feature is enabled (default)
    /// - `jiff::Timestamp` when `jiff-datetime` feature is enabled
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct DateTime(ChronoDateTime<Utc>);

    impl DateTime {
        /// Get current UTC datetime
        pub fn now() -> Self {
            DateTime(Utc::now())
        }

        /// Create from Unix timestamp in milliseconds
        pub fn from_millis(millis: i64) -> Option<Self> {
            use chrono::TimeZone;
            match Utc.timestamp_millis_opt(millis) {
                chrono::LocalResult::Single(dt) => Some(DateTime(dt)),
                _ => None,
            }
        }

        /// Convert to Unix timestamp in milliseconds
        pub fn as_millis(&self) -> i64 {
            self.0.timestamp_millis()
        }

        /// Format using strftime format string
        pub fn format(&self, fmt: &str) -> String {
            self.0.format(fmt).to_string()
        }
    }

    impl fmt::Display for DateTime {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

#[cfg(feature = "jiff-datetime")]
mod imp {
    use super::*;
    pub use jiff::Timestamp;

    /// UTC datetime type
    ///
    /// This type wraps either:
    /// - `chrono::DateTime<Utc>` when `chrono` feature is enabled (default)
    /// - `jiff::Timestamp` when `jiff-datetime` feature is enabled
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct DateTime(Timestamp);

    impl DateTime {
        /// Get current UTC datetime
        pub fn now() -> Self {
            DateTime(Timestamp::now())
        }

        /// Create from Unix timestamp in milliseconds
        pub fn from_millis(millis: i64) -> Option<Self> {
            match Timestamp::from_millisecond(millis) {
                Ok(ts) => Some(DateTime(ts)),
                Err(_) => None,
            }
        }

        /// Convert to Unix timestamp in milliseconds
        pub fn as_millis(&self) -> i64 {
            self.0.as_millisecond()
        }

        /// Format using strftime format string
        pub fn format(&self, fmt: &str) -> String {
            use jiff::fmt::strtime;
            let civil = self.0.to_zoned(jiff::tz::TimeZone::UTC).datetime();
            strtime::format(fmt, civil).unwrap_or_default()
        }
    }

    impl fmt::Display for DateTime {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
}

pub use imp::DateTime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_now() {
        let dt = DateTime::now();
        // Just verify it doesn't panic and returns a valid datetime
        assert!(dt.as_millis() > 0);
    }

    #[test]
    fn test_datetime_from_millis_roundtrip() {
        let original_millis = 1_700_000_000_000i64; // Some timestamp in 2023
        let dt = DateTime::from_millis(original_millis).expect("Valid timestamp");
        let recovered_millis = dt.as_millis();
        assert_eq!(original_millis, recovered_millis);
    }

    #[test]
    fn test_datetime_from_millis_invalid() {
        // Test with an invalid/overflowing timestamp
        let result = DateTime::from_millis(i64::MAX);
        assert!(result.is_none());
    }

    #[test]
    fn test_datetime_format() {
        // Use a known timestamp and verify format produces expected patterns
        // Note: chrono and jiff formatters may have slight differences in output
        let dt = DateTime::from_millis(1_705_318_200_000i64).expect("Valid timestamp");

        // Test that format produces non-empty strings with expected patterns
        let formatted = dt.format("%Y-%m-%d %H:%M:%S");
        assert!(!formatted.is_empty());
        assert!(formatted.contains("2024")); // Year should be present
        assert!(formatted.contains("15")); // Day should be present

        // Test date format
        let formatted_date = dt.format("%m/%d/%Y");
        assert!(!formatted_date.is_empty());
        assert!(formatted_date.contains("2024")); // Year should be present

        // Test time format with 12-hour clock
        let formatted_time = dt.format("%I:%M %p");
        assert!(!formatted_time.is_empty());
        assert!(formatted_time.contains('M')); // AM/PM indicator
    }

    #[test]
    fn test_datetime_display() {
        let dt = DateTime::now();
        let display_str = format!("{}", dt);
        // Just verify it produces a non-empty string
        assert!(!display_str.is_empty());
    }

    #[test]
    fn test_datetime_ordering() {
        let dt1 = DateTime::from_millis(1_000_000_000_000i64).expect("Valid timestamp");
        let dt2 = DateTime::from_millis(2_000_000_000_000i64).expect("Valid timestamp");

        assert!(dt1 < dt2);
        assert!(dt2 > dt1);
        assert_eq!(dt1, dt1);
    }

    #[test]
    #[cfg(feature = "serde_json")]
    fn test_datetime_serde_roundtrip() {
        use serde_json;
        let original = DateTime::from_millis(1_700_000_000_000i64).expect("Valid timestamp");

        // Serialize to JSON
        let json = serde_json::to_string(&original).expect("Serialization failed");

        // Deserialize back
        let deserialized: DateTime = serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(original.as_millis(), deserialized.as_millis());
    }
}
