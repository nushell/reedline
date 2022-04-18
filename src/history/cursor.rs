use crate::{History, HistoryNavigationQuery};

use super::base::CommandLineSearch;
use super::base::SearchDirection;
use super::base::SearchFilter;
use super::HistoryItem;
use super::Result;
use super::SearchQuery;

/// Interface of a stateful navigation via [`HistoryNavigationQuery`].
#[derive(Debug)]
pub struct HistoryCursor {
    query: HistoryNavigationQuery,
    current: Option<HistoryItem>,
    skip_dupes: bool,
}

impl HistoryCursor {
    pub fn new(query: HistoryNavigationQuery) -> HistoryCursor {
        HistoryCursor {
            query,
            current: None,
            skip_dupes: true,
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
            HistoryNavigationQuery::Normal(_) => SearchFilter::anything(),
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                SearchFilter::from_text_search(CommandLineSearch::Prefix(prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                SearchFilter::from_text_search(CommandLineSearch::Substring(substring))
            }
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
        #[cfg(feature = "sqlite")]
        let hist = Box::new(SqliteBackedHistory::in_memory().unwrap());
        #[cfg(not(feature = "sqlite"))]
        let hist = Box::new(FileBackedHistory::default());
        (
            hist,
            HistoryCursor::new(HistoryNavigationQuery::Normal(LineBuffer::default())),
        )
    }
    fn create_history_at(cap: usize, path: &Path) -> (Box<dyn History>, HistoryCursor) {
        let hist = Box::new(FileBackedHistory::with_file(cap, path.to_owned()).unwrap());
        (
            hist,
            HistoryCursor::new(HistoryNavigationQuery::Normal(LineBuffer::default())),
        )
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

    #[cfg(not(feature = "sqlite"))]
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

    #[cfg(not(feature = "sqlite"))]
    #[test]
    fn appends_no_empties() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line(""))?;
        assert_eq!(hist.count_all()?, 0);
        Ok(())
    }

    #[test]
    fn prefix_search_works() -> Result<()> {
        let (mut hist, _) = create_history();
        hist.save(HistoryItem::from_command_line("find me as well"))?;
        hist.save(HistoryItem::from_command_line("test"))?;
        hist.save(HistoryItem::from_command_line("find me"))?;

        let mut cursor =
            HistoryCursor::new(HistoryNavigationQuery::PrefixSearch("find".to_string()));

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

        let mut cursor =
            HistoryCursor::new(HistoryNavigationQuery::PrefixSearch("find".to_string()));
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

        let mut cursor =
            HistoryCursor::new(HistoryNavigationQuery::PrefixSearch("find".to_string()));
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

        let mut cursor =
            HistoryCursor::new(HistoryNavigationQuery::PrefixSearch("find".to_string()));
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

        let mut cursor =
            HistoryCursor::new(HistoryNavigationQuery::PrefixSearch("find".to_string()));
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

        let mut cursor = HistoryCursor::new(HistoryNavigationQuery::SubstringSearch(
            "substring".to_string(),
        ));
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

        let cursor = HistoryCursor::new(HistoryNavigationQuery::SubstringSearch("".to_string()));

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

            entries.iter().for_each(|e| {
                hist.save(HistoryItem::from_command_line(*e)).unwrap();
            });

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let (reading_hist, _) = create_history_at(5, &histfile);
        let res = reading_hist.search(SearchQuery::everything(SearchDirection::Forward))?;
        let actual: Vec<_> = res.iter().map(|e| &e.command_line).collect();
        assert_eq!(entries, actual);

        tmp.close().unwrap();
        Ok(())
    }
    /*
       #[test]
       fn persists_newlines_in_entries() -> Result<()> {
           use tempfile::tempdir;

           let tmp = tempdir()?;
           let histfile = tmp.path().join(".history");

           let entries = vec![
               "test",
               "multiline\nentry\nunix",
               "multiline\r\nentry\r\nwindows",
               "more test text",
           ];

           {
           let (mut hist, mut cursor) = create_history();

               entries.iter().for_each(|e| writing_hist.save(HistoryItem::from_command_line(e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           let (mut hist, mut cursor) = create_history();

           let actual: Vec<_> = reading_hist.iter_chronologic().collect();
           assert_eq!(entries, actual);

           tmp.close()?;
     Ok(())  }

       #[test]
       fn truncates_file_to_capacity() -> Result<()> {
           use tempfile::tempdir;

           let tmp = tempdir()?;
           let histfile = tmp.path().join(".history");

           let capacity = 5;
           let initial_entries = vec!["test 1", "test 2"];
           let appending_entries = vec!["test 3", "test 4"];
           let expected_appended_entries = vec!["test 1", "test 2", "test 3", "test 4"];
           let truncating_entries = vec!["test 5", "test 6", "test 7", "test 8"];
           let expected_truncated_entries = vec!["test 4", "test 5", "test 6", "test 7", "test 8"];

           {
               let mut writing_hist =
                   FileBackedHistory::with_file(capacity, histfile.clone())?;

               initial_entries.iter().for_each(|e| writing_hist.save(HistoryItem::from_command_line(e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           {
               let mut appending_hist =
                   FileBackedHistory::with_file(capacity, histfile.clone())?;

               appending_entries
                   .iter()
                   .for_each(|e| appending_hist.save(HistoryItem::from_command_line(e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
               let actual: Vec<_> = appending_hist.iter_chronologic().collect();
               assert_eq!(expected_appended_entries, actual);
           }

           {
               let mut truncating_hist =
                   FileBackedHistory::with_file(capacity, histfile.clone())?;

               truncating_entries
                   .iter()
                   .for_each(|e| truncating_hist.save(HistoryItem::from_command_line(e)))?;

               let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
               assert_eq!(expected_truncated_entries, actual);
               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           let (mut hist, mut cursor) = create_history();

           let actual: Vec<_> = reading_hist.iter_chronologic().collect();
           assert_eq!(expected_truncated_entries, actual);

           tmp.close()?;
    Ok(())   }

       #[test]
       fn truncates_too_large_file() -> Result<()> {
           use tempfile::tempdir;

           let tmp = tempdir()?;
           let histfile = tmp.path().join(".history");

           let overly_large_previous_entries = vec![
               "test 1", "test 2", "test 3", "test 4", "test 5", "test 6", "test 7", "test 8",
           ];
           let expected_truncated_entries = vec!["test 4", "test 5", "test 6", "test 7", "test 8"];

           {
           let (mut hist, mut cursor) = create_history();

               overly_large_previous_entries
                   .iter()
                   .for_each(|e| writing_hist.save(HistoryItem::from_command_line(e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           {
           let (mut hist, mut cursor) = create_history();

               let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
               assert_eq!(expected_truncated_entries, actual);
               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           let (mut hist, mut cursor) = create_history();

           let actual: Vec<_> = reading_hist.iter_chronologic().collect();
           assert_eq!(expected_truncated_entries, actual);

           tmp.close()?;
    Ok(())   }

       #[test]
       fn concurrent_histories_dont_erase_eachother() -> Result<()> {
           use tempfile::tempdir;

           let tmp = tempdir()?;
           let histfile = tmp.path().join(".history");

           let capacity = 7;
           let initial_entries = vec!["test 1", "test 2", "test 3", "test 4", "test 5"];
           let entries_a = vec!["A1", "A2", "A3"];
           let entries_b = vec!["B1", "B2", "B3"];
           let expected_entries = vec!["test 5", "B1", "B2", "B3", "A1", "A2", "A3"];

           {
               let mut writing_hist =
                   FileBackedHistory::with_file(capacity, histfile.clone())?;

               initial_entries.iter().for_each(|e| writing_hist.save(HistoryItem::from_command_line(e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           {
           let (mut hist, mut cursor) = create_history();

               {
           let (mut hist, mut cursor) = create_history();

                   entries_b.iter().for_each(|e| hist_b.append(e));

                   // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
               }
               entries_a.iter().for_each(|e| hist_a.append(e));

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           let (mut hist, mut cursor) = create_history();

           let actual: Vec<_> = reading_hist.iter_chronologic().collect();
           assert_eq!(expected_entries, actual);

           tmp.close()?;
     Ok(())  }

       #[test]
       fn concurrent_histories_are_threadsafe() -> Result<()> {
           use tempfile::tempdir;

           let tmp = tempdir()?;
           let histfile = tmp.path().join(".history");

           let num_threads = 16;
           let capacity = 2 * num_threads + 1;

           let initial_entries = (0..capacity).map(|i| format!("initial {i}"));

           {
               let mut writing_hist =
                   FileBackedHistory::with_file(capacity, histfile.clone())?;

               initial_entries.for_each(|e| writing_hist.save(HistoryItem::from_command_line(&e)))?;

               // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
           }

           let threads = (0..num_threads)
               .map(|i| {
                   let cap = capacity;
                   let hfile = histfile.clone();
                   std::thread::spawn(move || {
           let (mut hist, mut cursor) = create_history();
                       hist.save(HistoryItem::from_command_line(&format!("A{}", i)))?;
                       hist.sync()?;
                       hist.save(HistoryItem::from_command_line(&format!("B{}", i)))?;
                   })
               })
               .collect::<Vec<_>>();

           for t in threads {
               t.join()?;
           }

           let (mut hist, mut cursor) = create_history();

           let actual: Vec<_> = reading_hist.iter_chronologic().collect();

           assert!(
               actual.contains(&&format!("initial {}", capacity - 1)),
               "Overwrote entry from before threading test"
           );

           for i in 0..num_threads {
               assert!(actual.contains(&&format!("A{}", i)),);
               assert!(actual.contains(&&format!("B{}", i)),);
           }

           tmp.close()?;
     Ok(())  }*/
}
