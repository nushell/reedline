use std::collections::vec_deque::Iter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryNavigationQuery {
    Normal,
    PrefixSearch(String),
    SubstringSearch(String),
    // Suffix Search
    // Fuzzy Search
}

pub trait HistoryAppender {
    // append any given string (a command) into the history - store
    fn append(&mut self, entry: String);

    fn iter_chronologic(&self) -> Iter<'_, String>;
}

pub trait HistoryView {
    // This moves the cursor backwards respecting the navigation query that is set
    // - Results in a no-op if the cursor is at the initial point
    fn back(&mut self);

    // This moves the cursor forwards respecting the navigation-query that is set
    // - Results in a no-op if the cursor is at the latest point
    fn forward(&mut self);

    // Returns the string (if present) at the cursor
    fn string_at_cursor(&self) -> Option<String>;

    // This will set a new navigation setup and based on input query
    fn set_navigation(&mut self, navigation: HistoryNavigationQuery);

    fn get_navigation(&self) -> HistoryNavigationQuery;
}

pub trait History: HistoryAppender + HistoryView {}
