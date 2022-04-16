use std::time::Duration;

use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};

use crate::core_editor::LineBuffer;

// todo: better error type
pub type Result<T> = std::result::Result<T, String>;

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

#[derive(Copy, Clone)]
pub struct HistoryItemId(pub(crate) i64);
impl HistoryItemId {
    pub fn new(i: i64) -> HistoryItemId {
        HistoryItemId(i)
    }
}
/// This trait represents additional context to be added to a history (see [HistoryItem])
pub trait HistoryEntryContext: Serialize + DeserializeOwned + Default + Send {}
impl HistoryEntryContext for () {}
#[derive(Clone)]
pub struct HistoryItem<ExtraInfo: HistoryEntryContext = ()> {
    pub id: Option<HistoryItemId>,
    pub start_timestamp: chrono::DateTime<Utc>,
    pub command_line: String,
    pub session_id: Option<i64>,
    pub hostname: Option<String>,
    pub cwd: Option<String>,
    pub duration: Option<Duration>,
    pub exit_status: Option<i64>,
    pub more_info: Option<ExtraInfo>, // pub more_info: Option<Box<dyn HistoryEntryContext>>
}

impl Default for HistoryItem {
    fn default() -> HistoryItem {
        todo!()
    }
}

pub enum SearchDirection {
    Backward,
    Forward,
}

pub enum CommandLineSearch {
    Prefix(String),
    Substring(String),
    Exact(String),
}

pub struct SearchFilter {
    pub command_line: Option<CommandLineSearch>,
    pub hostname: Option<String>,
    pub cwd_exact: Option<String>,
    pub cwd_prefix: Option<String>,
    pub exit_successful: Option<bool>,
}

pub trait History {
    // returns the same history item but with the correct id set
    fn save(&mut self, h: HistoryItem) -> Result<HistoryItem>;
    fn load(&mut self, id: HistoryItemId) -> Result<HistoryItem>;
    fn search(
        &self,
        start: chrono::DateTime<Utc>,
        direction: SearchDirection,
        end: Option<chrono::DateTime<Utc>>,
        limit: Option<i64>,
        filter: SearchFilter,
    ) -> Result<Vec<HistoryItem>>;

    fn update(
        &mut self,
        id: HistoryItemId,
        updater: Box<dyn FnOnce(HistoryItem) -> HistoryItem>,
    ) -> Result<()>;

    fn entry_count(&self) -> Result<i64>;
    fn delete(&mut self, h: HistoryItemId) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
}

/// Interface of a history datastructure that supports stateful navigation via [`HistoryNavigationQuery`].
pub trait HistoryCursor: Send {
    /// Chronologic interaction over all entries present in the history
    fn iter_chronologic(&self) -> Box<dyn DoubleEndedIterator<Item = String> + '_>;

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

    /// Synchronize the state of the history with the backing filesystem or database if available
    fn sync(&mut self) -> std::io::Result<()>;

    /// Reset the browsing cursor back outside the history, does not affect the [`HistoryNavigationQuery`]
    fn reset_cursor(&mut self);
}
