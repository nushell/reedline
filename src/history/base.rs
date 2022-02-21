use crate::core_editor::LineBuffer;
use std::collections::vec_deque::Iter;
use time::error::InvalidFormatDescription;
use time::format_description::FormatItem;
use time::{format_description, OffsetDateTime};

/// Browsing modes for a [`History`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryNavigationQuery {
    /// `bash` style browsing through the history. Contained `LineBuffer` is used to store the state of manual entry before browsing through the history
    Normal(LineBuffer),
    /// Search for entries starting with a particular string.
    PrefixSearch(String),
    /// Full exact search for all entries containing a string.
    SubstringSearch(String),
    // Suffix Search
    // Fuzzy Search
}

/// Stateful history entry. Record about history message and operation time.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub(crate) time: OffsetDateTime,
    pub(crate) entry: String,
    pub(crate) index: bool,
}

impl HistoryEntry {
    /// Construct a new `HistoryEntry`
    pub(crate) fn new<I: Into<String>>(time: OffsetDateTime, entry: I) -> Self {
        Self {
            time,
            entry: entry.into(),
            index: false,
        }
    }

    /// format [`HistoryEntry`] output
    pub(crate) fn format(&self, i: usize, f: Option<FormatTimeType>) -> anyhow::Result<String> {
        let format = if let Some(f) = f {
            let format_str = match f {
                FormatTimeType::Time(_) => self.time.time().format(&f.format_item()?)?,
                FormatTimeType::DateTime(_) => self.time.format(&f.format_item()?)?,
            };
            format!("{}\t{}", format_str, self.entry)
        } else {
            format!("{}", self.entry)
        };
        if self.index {
            return Ok(format!("{}\t{}", i + 1, format));
        }
        Ok(format)
    }
}

/// With [`FormatTimeType`] get format time type
///
/// The syntax for the format description can be found in [the
/// book](https://time-rs.github.io/book/api/format-description.html).#[derive(Clone, Debug)]
///
#[derive(Debug, Clone)]
pub enum FormatTimeType {
    /// use [time::format_description] with format [`Time`].
    /// ```rust, no_run
    ///  use reedline::FormatTimeType;
    /// FormatTimeType::Time("[hour]:[minute]:[second]".to_string());
    /// ```
    Time(String),

    /// use [time::format_description] with format [`DateTime`].
    /// ```rust, no_run
    /// use reedline::FormatTimeType;
    /// FormatTimeType::DateTime(
    ///     "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour \
    ///          sign:mandatory]:[offset_minute]:[offset_second]".to_string());
    /// ```
    DateTime(String),
}

impl FormatTimeType {
    /// Validate format time type
    pub(crate) fn validate(&self) -> Result<(), InvalidFormatDescription> {
        self.format_item().and_then(|_| Ok(()))
    }

    /// format [`FormatTimeType`] output with parse.
    pub(crate) fn format_item(&self) -> Result<Vec<FormatItem<'_>>, InvalidFormatDescription> {
        match self {
            FormatTimeType::Time(f) | FormatTimeType::DateTime(f) => {
                let vec = format_description::parse(f)?;
                Ok(vec)
            }
        }
    }
}

/// Interface of a history datastructure that supports stateful navigation via [`HistoryNavigationQuery`].
pub trait History: Send {
    /// Append entry to the history, if capacity management is part of the implementation may perform that as well
    fn append(&mut self, entry: &str);

    /// Returns the [`FormatTimeType`] (if present)
    fn format_time_type(&self) -> Option<FormatTimeType>;
    /// Chronologic interaction over all entries present in the history
    fn iter_chronologic(&self) -> Iter<'_, HistoryEntry>;

    /// This moves the cursor backwards respecting the navigation query that is set
    /// - Results in a no-op if the cursor is at the initial point
    fn back(&mut self);

    /// This moves the cursor forwards respecting the navigation-query that is set
    /// - Results in a no-op if the cursor is at the latest point
    fn forward(&mut self);

    /// Returns the string (if present) at the cursor
    fn string_at_cursor(&self) -> Option<String>;

    /// Set a new navigation mode for search based on input query defined in [`HistoryNavigationQuery`]
    ///
    /// By current convention, resets the position in the stateful browsing to the default.
    fn set_navigation(&mut self, navigation: HistoryNavigationQuery);

    /// Poll the current [`HistoryNavigationQuery`] mode
    fn get_navigation(&self) -> HistoryNavigationQuery;

    /// Query the values in the history entries
    fn query_entries(&self, search: &str) -> Vec<String>;

    /// Max number of values that can be queried from the history
    fn max_values(&self) -> usize;

    /// Appends an element to the back
    fn push_back(&mut self, entry: &str);

    /// Removes the first element and returns it, or None if this is empty.
    fn pop_front(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn test_history_entry() {
        let time = datetime!(2022-02-11 03:12 UTC);
        let entry = HistoryEntry::new(time, "hello reedline");
        let format = entry
            .format(
                0,
                Some(FormatTimeType::Time("[hour]:[minute]:[second]".to_string())),
            )
            .unwrap();
        assert_eq!(&format, "03:12:00	hello reedline");
    }
}
