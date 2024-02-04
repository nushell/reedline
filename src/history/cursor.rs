use crate::{History, HistoryNavigationQuery, HistorySessionId};

use super::base::CommandLineSearch;
use super::base::SearchDirection;
use super::base::SearchFilter;
use super::HistoryItem;
use super::SearchQuery;
use crate::Result;

/// Interface of a stateful navigation via [`HistoryNavigationQuery`].
#[derive(Debug)]
pub struct HistoryCursor {
    query: HistoryNavigationQuery,
    current: Option<HistoryItem>,
    skip_dupes: bool,
    session: Option<HistorySessionId>,
}

impl HistoryCursor {
    pub fn new(query: HistoryNavigationQuery, session: Option<HistorySessionId>) -> HistoryCursor {
        HistoryCursor {
            query,
            current: None,
            skip_dupes: true,
            session,
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

    fn get_search_filter(&self) -> SearchFilter {
        let filter = match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => SearchFilter::anything(self.session),
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                SearchFilter::from_text_search(CommandLineSearch::Prefix(prefix), self.session)
            }
            HistoryNavigationQuery::SubstringSearch(substring) => SearchFilter::from_text_search(
                CommandLineSearch::Substring(substring),
                self.session,
            ),
        };
        if let (true, Some(current)) = (self.skip_dupes, &self.current) {
            SearchFilter {
                not_command_line: Some(current.command_line.clone()),
                ..filter
            }
        } else {
            filter
        }
    }
    fn navigate_in_direction(
        &mut self,
        history: &dyn History,
        direction: SearchDirection,
    ) -> Result<()> {
        if direction == SearchDirection::Forward && self.current.is_none() {
            // if searching forward but we don't have a starting point, assume we are at the end
            return Ok(());
        }
        let start_id = self.current.as_ref().and_then(|e| e.id);
        let mut next = history.search(SearchQuery {
            start_id,
            end_id: None,
            start_time: None,
            end_time: None,
            direction,
            limit: Some(1),
            filter: self.get_search_filter(),
        })?;
        if next.len() == 1 {
            self.current = Some(next.swap_remove(0));
        } else if direction == SearchDirection::Forward {
            // no result and searching forward: we are at the end
            self.current = None;
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use crate::LineBuffer;

    use super::super::*;
    use super::*;

    fn create_history() -> (Box<dyn History>, HistoryCursor) {
        #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
        let hist = Box::new(SqliteBackedHistory::in_memory().unwrap());
        #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
        let hist = Box::<FileBackedHistory>::default();
        (
            hist,
            HistoryCursor::new(HistoryNavigationQuery::Normal(LineBuffer::default()), None),
        )
    }
    fn create_history_at(cap: usize, path: &Path) -> (Box<dyn History>, HistoryCursor) {
        let hist = Box::new(FileBackedHistory::with_file(cap, path.to_owned()).unwrap());
        (
            hist,
            HistoryCursor::new(HistoryNavigationQuery::Normal(LineBuffer::default()), None),
        )
    }

    fn get_all_entry_texts(hist: &dyn History) -> Vec<String> {
        let res = hist
            .search(SearchQuery::everything(SearchDirection::Forward, None))
            .unwrap();
        let actual: Vec<_> = res.iter().map(|e| e.command_line.to_string()).collect();
        actual
    }
    fn add_text_entries(hist: &mut dyn History, entries: &[impl AsRef<str>]) {
        entries.iter().for_each(|e| {
            hist.save(HistoryItem::from_command_line(e.as_ref()))
                .unwrap();
        });
    }

    #[test]
    fn accessing_empty_history_returns_nothing() -> Result<()> {
        let (_hist, cursor) = create_history();
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn going_forward_in_empty_history_does_not_error_out() -> Result<()> {
        let (hist, mut cursor) = create_history();
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn going_backwards_in_empty_history_does_not_error_out() -> Result<()> {
        let (hist, mut cursor) = create_history();
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn going_backwards_bottoms_out() -> Result<()> {
        let (mut hist, mut cursor) = create_history();
        hist.save(HistoryItem::from_command_line("command1"))?;
        hist.save(HistoryItem::from_command_line("command2"))?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("command1".to_string()));
        Ok(())
    }

    #[test]
    fn going_forwards_bottoms_out() -> Result<()> {
        let (mut hist, mut cursor) = create_history();
        hist.save(HistoryItem::from_command_line("command1"))?;
        hist.save(HistoryItem::from_command_line("command2"))?;
        cursor.forward(&*hist)?;
        cursor.forward(&*hist)?;
        cursor.forward(&*hist)?;
        cursor.forward(&*hist)?;
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
    #[test]
    fn appends_only_unique() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("unique_old"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("unique"))?;
        assert_eq!(hist.count_all()?, 3);
        Ok(())
    }

    #[test]
    fn prefix_search_works() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me as well"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::PrefixSearch("find".to_string()),
            None,
        );

        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me".to_string()));
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        Ok(())
    }

    #[test]
    fn prefix_search_bottoms_out() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me as well"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::PrefixSearch("find".to_string()),
            None,
        );
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me".to_string()));
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        Ok(())
    }
    #[test]
    fn prefix_search_returns_to_none() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me as well"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::PrefixSearch("find".to_string()),
            None,
        );
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me".to_string()));
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me".to_string()));
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn prefix_search_ignores_consecutive_equivalent_entries_going_backwards() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me as well"))?;
        hist.save(HistoryItem::from_command_line("find me once"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me once"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::PrefixSearch("find".to_string()),
            None,
        );
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me once".to_string()));
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        Ok(())
    }

    #[test]
    fn prefix_search_ignores_consecutive_equivalent_entries_going_forwards() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me once"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me once"))?;
        hist.save(HistoryItem::from_command_line("find me as well"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::PrefixSearch("find".to_string()),
            None,
        );
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        cursor.back(&*hist)?;
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("find me once".to_string()));
        cursor.forward(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("find me as well".to_string())
        );
        cursor.forward(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn substring_search_works() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("substring"))?;
        hist.save(HistoryItem::from_command_line("don't find me either"))?;
        hist.save(HistoryItem::from_command_line("prefix substring"))?;
        hist.save(HistoryItem::from_command_line("don't find me"))?;
        hist.save(HistoryItem::from_command_line("prefix substring suffix"))?;

        let mut cursor = HistoryCursor::new(
            HistoryNavigationQuery::SubstringSearch("substring".to_string()),
            None,
        );
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("prefix substring suffix".to_string())
        );
        cursor.back(&*hist)?;
        assert_eq!(
            cursor.string_at_cursor(),
            Some("prefix substring".to_string())
        );
        cursor.back(&*hist)?;
        assert_eq!(cursor.string_at_cursor(), Some("substring".to_string()));
        Ok(())
    }

    #[test]
    fn substring_search_with_empty_value_returns_none() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("substring"))?;

        let cursor = HistoryCursor::new(
            HistoryNavigationQuery::SubstringSearch("".to_string()),
            None,
        );

        assert_eq!(cursor.string_at_cursor(), None);
        Ok(())
    }

    #[test]
    fn writes_to_new_file() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        // check that it also works for a path where the directory has not been created yet
        let histfile = tmp.path().join("nested_path").join(".history");

        let entries = vec!["test", "text", "more test text"];

        {
            let (mut hist, _) = create_history_at(5, &histfile);

            add_text_entries(hist.as_mut(), &entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(5, &histfile);
        let actual = get_all_entry_texts(reading_hist.as_ref());
        assert_eq!(entries, actual);

        tmp.close().unwrap();
        Ok(())
    }

    #[test]
    fn persists_newlines_in_entries() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let entries = vec![
            "test",
            "multiline\nentry\nunix",
            "multiline\r\nentry\r\nwindows",
            "more test text",
        ];

        {
            let (mut writing_hist, _) = create_history_at(5, &histfile);
            add_text_entries(writing_hist.as_mut(), &entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(5, &histfile);

        let actual: Vec<_> = get_all_entry_texts(reading_hist.as_ref());
        assert_eq!(entries, actual);

        tmp.close().unwrap();
        Ok(())
    }

    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
    #[test]
    fn truncates_file_to_capacity() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let capacity = 5;
        let initial_entries = vec!["test 1", "test 2"];
        let appending_entries = vec!["test 3", "test 4"];
        let expected_appended_entries = vec!["test 1", "test 2", "test 3", "test 4"];
        let truncating_entries = vec!["test 5", "test 6", "test 7", "test 8"];
        let expected_truncated_entries = vec!["test 4", "test 5", "test 6", "test 7", "test 8"];

        {
            let (mut writing_hist, _) = create_history_at(capacity, &histfile);
            add_text_entries(writing_hist.as_mut(), &initial_entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let (mut appending_hist, _) = create_history_at(capacity, &histfile);
            add_text_entries(appending_hist.as_mut(), &appending_entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            let actual: Vec<_> = get_all_entry_texts(appending_hist.as_ref());
            assert_eq!(expected_appended_entries, actual);
        }

        {
            let (mut truncating_hist, _) = create_history_at(capacity, &histfile);
            add_text_entries(truncating_hist.as_mut(), &truncating_entries);
            let actual: Vec<_> = get_all_entry_texts(truncating_hist.as_ref());
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(capacity, &histfile);

        let actual: Vec<_> = get_all_entry_texts(reading_hist.as_ref());
        assert_eq!(expected_truncated_entries, actual);

        tmp.close().unwrap();
        Ok(())
    }

    #[test]
    fn truncates_too_large_file() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let overly_large_previous_entries = vec![
            "test 1", "test 2", "test 3", "test 4", "test 5", "test 6", "test 7", "test 8",
        ];
        let expected_truncated_entries = vec!["test 4", "test 5", "test 6", "test 7", "test 8"];

        {
            let (mut writing_hist, _) = create_history_at(10, &histfile);
            add_text_entries(writing_hist.as_mut(), &overly_large_previous_entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let (truncating_hist, _) = create_history_at(5, &histfile);

            let actual: Vec<_> = get_all_entry_texts(truncating_hist.as_ref());
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(5, &histfile);

        let actual: Vec<_> = get_all_entry_texts(reading_hist.as_ref());
        assert_eq!(expected_truncated_entries, actual);

        tmp.close().unwrap();
        Ok(())
    }

    #[test]
    fn concurrent_histories_do_not_erase_each_other() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let capacity = 7;
        let initial_entries = vec!["test 1", "test 2", "test 3", "test 4", "test 5"];
        let entries_a = vec!["A1", "A2", "A3"];
        let entries_b = vec!["B1", "B2", "B3"];
        let expected_entries = vec!["test 5", "B1", "B2", "B3", "A1", "A2", "A3"];

        {
            let (mut writing_hist, _) = create_history_at(capacity, &histfile);
            add_text_entries(writing_hist.as_mut(), &initial_entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let (mut hist_a, _) = create_history_at(capacity, &histfile);

            {
                let (mut hist_b, _) = create_history_at(capacity, &histfile);

                add_text_entries(hist_b.as_mut(), &entries_b);
                // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            }
            add_text_entries(hist_a.as_mut(), &entries_a);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(capacity, &histfile);

        let actual: Vec<_> = get_all_entry_texts(reading_hist.as_ref());
        assert_eq!(expected_entries, actual);

        tmp.close().unwrap();
        Ok(())
    }

    #[test]
    fn concurrent_histories_are_threadsafe() -> Result<()> {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let num_threads = 16;
        let capacity = 2 * num_threads + 1;

        let initial_entries: Vec<_> = (0..capacity).map(|i| format!("initial {i}")).collect();

        {
            let (mut writing_hist, _) = create_history_at(capacity, &histfile);
            add_text_entries(writing_hist.as_mut(), &initial_entries);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let threads = (0..num_threads)
            .map(|i| {
                let cap = capacity;
                let hfile = histfile.clone();
                std::thread::spawn(move || {
                    let (mut hist, _) = create_history_at(cap, &hfile);
                    hist.save(HistoryItem::from_command_line(format!("A{i}")))
                        .unwrap();
                    hist.sync().unwrap();
                    hist.save(HistoryItem::from_command_line(format!("B{i}")))
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();

        for t in threads {
            t.join().unwrap();
        }

        let (reading_hist, _) = create_history_at(capacity, &histfile);

        let actual: Vec<_> = get_all_entry_texts(reading_hist.as_ref());

        assert!(
            actual.contains(&format!("initial {}", capacity - 1)),
            "Overwrote entry from before threading test"
        );

        for i in 0..num_threads {
            assert!(actual.contains(&format!("A{i}")),);
            assert!(actual.contains(&format!("B{i}")),);
        }

        tmp.close().unwrap();
        Ok(())
    }
}
