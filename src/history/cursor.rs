use crate::{History, HistoryNavigationQuery};

use super::base::CommandLineSearch;
use super::base::SearchDirection;
use super::base::SearchFilter;
use super::HistoryItem;
use super::HistoryItemId;
use super::Result;
use super::SearchQuery;

/// Interface of a stateful navigation via [`HistoryNavigationQuery`].
#[derive(Debug)]
pub struct HistoryCursor {
    query: HistoryNavigationQuery,
    current: Option<HistoryItem>,
}

impl HistoryCursor {
    pub fn new(query: HistoryNavigationQuery) -> HistoryCursor {
        HistoryCursor {
            query,
            current: None,
        }
    }

    /// This moves the cursor backwards respecting the navigation query that is set
    /// - Results in a no-op if the cursor is at the initial point
    pub fn back(&mut self, history: &dyn History) -> Result<()> {
        self.navigate_in_direction(history, SearchDirection::Backward)
    }

    /// This moves the cursor forwards respecting the navigation-query that is set
    /// - Results in a no-op if the cursor is at the latest point
    pub fn forward(&mut self, history: &dyn History) -> Result<()> {
        self.navigate_in_direction(history, SearchDirection::Forward)
    }

    fn get_search_start(&self, history: &dyn History) -> Result<Option<HistoryItemId>> {
        if let Some(it) = &self.current {
            Ok(it.id)
        } else {
            history.newest()
        }
    }

    fn get_search_filter(&self) -> SearchFilter {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => SearchFilter::anything(),
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                SearchFilter::from_text_search(CommandLineSearch::Prefix(prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                SearchFilter::from_text_search(CommandLineSearch::Substring(substring))
            }
        }
    }
    fn navigate_in_direction(
        &mut self,
        history: &dyn History,
        direction: SearchDirection,
    ) -> Result<()> {
        if let Some(start_id) = self.get_search_start(history)? {
            let mut next = history.search(SearchQuery {
                start_id: Some(start_id),
                end_id: None,
                start_time: None,
                end_time: None,
                direction,
                limit: Some(1),
                filter: self.get_search_filter(),
            })?;
            if next.len() == 1 {
                self.current = Some(next.swap_remove(0));
            }
        }
        Ok(())
    }

    /// Returns the string (if present) at the cursor
    pub fn string_at_cursor(&self) -> Option<String> {
        self.current.as_ref().map(|e| e.command_line.to_string())
    }

    /// Poll the current [`HistoryNavigationQuery`] mode
    pub fn get_navigation(&self) -> HistoryNavigationQuery {
        self.query.clone()
    }

}
