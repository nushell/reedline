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

#[derive(Debug, Clone, PartialEq)]
pub struct InnerEntry {
    pub(crate) time: OffsetDateTime,
    pub(crate) entry: String,
}

impl InnerEntry {
    pub fn new<I: Into<String>>(time: OffsetDateTime, entry: I) -> Self {
        Self {
            time,
            entry: entry.into(),
        }
    }

    pub fn format(&self, i: usize, f: Option<FormatTimeType>) -> anyhow::Result<String> {
        if let Some(f) = f {
            let format_str = match f {
                FormatTimeType::Time(_) => self.time.time().format(&f.format_item()?)?,
                FormatTimeType::Date(_) => self.time.format(&f.format_item()?)?,
            };
            return Ok(format!("{}\t{}", format_str, self.entry));
        }
        Ok(format!("{}\t{}", i + 1, self.entry))
    }
}

#[derive(Clone, Debug)]
pub enum FormatTimeType {
    Time(String),
    Date(String),
}

impl FormatTimeType {
    pub(crate) fn validate_format(&self) -> Result<(), InvalidFormatDescription> {
        self.format_item().and_then(|_| Ok(()))
    }

    pub(crate) fn format_item(&self) -> Result<Vec<FormatItem<'_>>, InvalidFormatDescription> {
        match self {
            FormatTimeType::Time(f) | FormatTimeType::Date(f) => {
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

    fn format_time_type(&self) -> Option<FormatTimeType>;
    /// Chronologic interaction over all entries present in the history
    fn iter_chronologic(&self) -> Iter<'_, InnerEntry>;

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

    fn push_back(&mut self, entry: &str);
    fn pop_front(&mut self);
}
