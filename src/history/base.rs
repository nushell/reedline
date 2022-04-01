use crate::core_editor::LineBuffer;

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

/// Interface of a history datastructure that supports stateful navigation via [`HistoryNavigationQuery`].
pub trait History: Send {
    /// Append entry to the history, if capacity management is part of the implementation may perform that as well
    fn append(&mut self, entry: &str);

    /// Chronologic interaction over all entries present in the history
    fn iter_chronologic(&self) -> Box<dyn DoubleEndedIterator<Item=String> + '_>;

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
