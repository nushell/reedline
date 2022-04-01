use rusqlite::{named_params, params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{base::HistoryNavigationQuery, History};
use crate::core_editor::LineBuffer;
use std::{
    collections::{vec_deque::Iter, VecDeque},
    fs::OpenOptions,
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

pub trait HistoryEntryContext: Serialize + DeserializeOwned + Default + Send {}
impl<T> HistoryEntryContext for T where T: Serialize + DeserializeOwned + Default + Send {}

/// A history that stores the values to an SQLite database.
/// In addition to storing the command, the history can store an additional arbitrary HistoryEntryContext,
/// to add information such as a timestamp, running directory, result...
#[derive(Clone)]
pub struct SqliteBackedHistory<ContextType>
where
    ContextType: HistoryEntryContext,
{
    inner: Arc<Mutex<SqliteBackedHistoryInner<ContextType>>>,
    dummy: VecDeque<String>,
}
struct SqliteBackedHistoryInner<ContextType> {
    db: rusqlite::Connection,
    last_command_id: Option<i64>,
    last_command_context: Option<ContextType>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct CommandSqlite<'a> {
    id: i64,
    command: &'a str,
}
impl<ContextType: HistoryEntryContext> History for SqliteBackedHistory<ContextType> {
    /// Appends an entry if non-empty and not repetition of the previous entry.
    /// Resets the browsing cursor to the default state in front of the most recent entry.
    ///
    fn append(&mut self, entry: &str) {
        let mut inner = self.inner.lock().expect("lock poisoned");
        let ctx = ContextType::default();
        let ret: i64 = inner
            .db
            .prepare(
                "insert into history (command, context) values (:command, :context) returning id",
            )
            .unwrap()
            .query_row(
                named_params! {
                    ":command": entry,
                    ":context": serde_json::to_string(&ctx).unwrap()
                },
                |row| row.get(0),
            )
            .unwrap();
        inner.last_command_id = Some(ret);
        inner.last_command_context = Some(ctx);
    }

    fn iter_chronologic(&self) -> Iter<'_, String> {
        self.dummy.iter()
        // self.entries.iter()
    }

    fn back(&mut self) {
        todo!()
    }

    fn forward(&mut self) {
        todo!()
    }

    fn string_at_cursor(&self) -> Option<String> {
        todo!()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        todo!()
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        todo!()
    }

    fn query_entries(&self, search: &str) -> Vec<String> {
        self.iter_chronologic()
            .rev()
            .filter(|entry| entry.contains(search))
            .cloned()
            .collect::<Vec<String>>()
    }

    fn max_values(&self) -> usize {
        todo!()
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        todo!()
    }
}
fn map_sqlite_err(err: rusqlite::Error) -> std::io::Error {
    // todo: better error mapping
    std::io::Error::new(std::io::ErrorKind::Other, err)
}


impl<ContextType: HistoryEntryContext> SqliteBackedHistory<ContextType> {
    /*pub fn new() -> Self {
        with_file_for_test(":memory:")
    }*/

    /// Creates a new history with an associated history file.
    ///
    ///
    /// **Side effects:** creates all nested directories to the file
    ///
    pub fn with_file(file: PathBuf) -> std::io::Result<Self> {
        if let Some(base_dir) = file.parent() {
            std::fs::create_dir_all(base_dir)?;
        }
        let db = Connection::open(&file).map_err(map_sqlite_err)?;
        Self::from_connection(db)
    }
    /// Creates a new history in memory
    pub fn in_memory() -> std::io::Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(map_sqlite_err)?)
    }
    fn from_connection(db: Connection) -> std::io::Result<Self> {
        db.pragma_update(None, "journal_mode", "wal").map_err(map_sqlite_err)?;
        db.execute(
            "
        create table if not exists history (
            id integer primary key autoincrement,
            command text not null,
            context text not null
        ) strict
        ",
            params![],
        ).map_err(map_sqlite_err)?;
        Ok(SqliteBackedHistory {
            dummy: VecDeque::new(),
            inner: Arc::new(Mutex::new(SqliteBackedHistoryInner {
                db,
                last_command_id: None,
                last_command_context: None,
            })),
        })
    }

    // todo: better error type (which one?)
    /// updates the context stored for the last saved command
    pub fn update_context<F>(&self, callback: F) -> Result<(), String>
    where
        F: FnOnce(ContextType) -> ContextType,
    {
        let mut inner = self.inner.lock().expect("lock poisoned");
        if let (Some(id), Some(ctx)) = (inner.last_command_id, inner.last_command_context.take()) {
            let mapped_ctx = callback(ctx);
            inner
                .db
                .execute(
                    "update history set context = :context where id = :id",
                    named_params! {
                        ":context": serde_json::to_string(&mapped_ctx).map_err(|e| format!("{e}"))?,
                        ":id": id
                    },
                )
                .map_err(|e| format!("{e}"))?;
            inner.last_command_context.replace(mapped_ctx);
            Ok(())
        } else {
            Err(format!("No command has been executed yet"))
        }
    }
}
#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn in_memory_for_test() -> SqliteBackedHistory<()> {
        SqliteBackedHistory::in_memory().unwrap()
    }
    fn with_file_for_test(capacity: i32, file: PathBuf) -> std::io::Result<SqliteBackedHistory<()>> {
        SqliteBackedHistory::with_file(file)
    }

    #[test]
    fn accessing_empty_history_returns_nothing() {
        let hist = in_memory_for_test();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_forward_in_empty_history_does_not_error_out() {
        let mut hist = in_memory_for_test();
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_backwards_in_empty_history_does_not_error_out() {
        let mut hist = in_memory_for_test();
        hist.back();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_backwards_bottoms_out() {
        let mut hist = in_memory_for_test();
        hist.append("command1");
        hist.append("command2");
        hist.back();
        hist.back();
        hist.back();
        hist.back();
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("command1".to_string()));
    }

    #[test]
    fn going_forwards_bottoms_out() {
        let mut hist = in_memory_for_test();
        hist.append("command1");
        hist.append("command2");
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    /*#[test]
    fn appends_only_unique() {
        let mut hist = in_memory_for_test();
        hist.append("unique_old");
        hist.append("test");
        hist.append("test");
        hist.append("unique");
        assert_eq!(hist.entries.len(), 3);
    }
    #[test]
    fn appends_no_empties() {
        let mut hist = in_memory_for_test();
        hist.append("");
        assert_eq!(hist.entries.len(), 0);
    }*/

    #[test]
    fn prefix_search_works() {
        let mut hist = in_memory_for_test();
        hist.append("find me as well");
        hist.append("test");
        hist.append("find me");

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));

        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }

    #[test]
    fn prefix_search_bottoms_out() {
        let mut hist = in_memory_for_test();
        hist.append("find me as well");
        hist.append("test");
        hist.append("find me");

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
        hist.back();
        hist.back();
        hist.back();
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }
    #[test]
    fn prefix_search_returns_to_none() {
        let mut hist = in_memory_for_test();
        hist.append("find me as well");
        hist.append("test");
        hist.append("find me");

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
        hist.forward();
        assert_eq!(hist.string_at_cursor(), Some("find me".to_string()));
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn prefix_search_ignores_consecutive_equivalent_entries_going_backwards() {
        let mut hist = in_memory_for_test();
        hist.append("find me as well");
        hist.append("find me once");
        hist.append("test");
        hist.append("find me once");

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me once".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }

    #[test]
    fn prefix_search_ignores_consecutive_equivalent_entries_going_forwards() {
        let mut hist = in_memory_for_test();
        hist.append("find me once");
        hist.append("test");
        hist.append("find me once");
        hist.append("find me as well");

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));
        hist.back();
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me once".to_string()));
        hist.forward();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn substring_search_works() {
        let mut hist = in_memory_for_test();
        hist.append("substring");
        hist.append("don't find me either");
        hist.append("prefix substring");
        hist.append("don't find me");
        hist.append("prefix substring suffix");

        hist.set_navigation(HistoryNavigationQuery::SubstringSearch(
            "substring".to_string(),
        ));
        hist.back();
        assert_eq!(
            hist.string_at_cursor(),
            Some("prefix substring suffix".to_string())
        );
        hist.back();
        assert_eq!(
            hist.string_at_cursor(),
            Some("prefix substring".to_string())
        );
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("substring".to_string()));
    }

    #[test]
    fn substring_search_with_empty_value_returns_none() {
        let mut hist = in_memory_for_test();
        hist.append("substring");

        hist.set_navigation(HistoryNavigationQuery::SubstringSearch("".to_string()));

        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn writes_to_new_file() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        // check that it also works for a path where the directory has not been created yet
        let histfile = tmp.path().join("nested_path").join(".history");

        let entries = vec!["test", "text", "more test text"];

        {
            let mut hist = with_file_for_test(5, histfile.clone()).unwrap();

            entries.iter().for_each(|e| hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = with_file_for_test(5, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();
        assert_eq!(entries, actual);

        tmp.close().unwrap();
    }

    #[test]
    fn persists_newlines_in_entries() {
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
            let mut writing_hist = with_file_for_test(5, histfile.clone()).unwrap();

            entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = with_file_for_test(5, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();
        assert_eq!(entries, actual);

        tmp.close().unwrap();
    }

    #[test]
    fn truncates_file_to_capacity() {
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
            let mut writing_hist =
                with_file_for_test(capacity, histfile.clone()).unwrap();

            initial_entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let mut appending_hist =
                with_file_for_test(capacity, histfile.clone()).unwrap();

            appending_entries
                .iter()
                .for_each(|e| appending_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            let actual: Vec<_> = appending_hist.iter_chronologic().collect();
            assert_eq!(expected_appended_entries, actual);
        }

        {
            let mut truncating_hist =
                with_file_for_test(capacity, histfile.clone()).unwrap();

            truncating_entries
                .iter()
                .for_each(|e| truncating_hist.append(e));

            let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = with_file_for_test(capacity, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();
        assert_eq!(expected_truncated_entries, actual);

        tmp.close().unwrap();
    }

    #[test]
    fn truncates_too_large_file() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let overly_large_previous_entries = vec![
            "test 1", "test 2", "test 3", "test 4", "test 5", "test 6", "test 7", "test 8",
        ];
        let expected_truncated_entries = vec!["test 4", "test 5", "test 6", "test 7", "test 8"];

        {
            let mut writing_hist = with_file_for_test(10, histfile.clone()).unwrap();

            overly_large_previous_entries
                .iter()
                .for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let truncating_hist = with_file_for_test(5, histfile.clone()).unwrap();

            let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = with_file_for_test(5, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();
        assert_eq!(expected_truncated_entries, actual);

        tmp.close().unwrap();
    }

    #[test]
    fn concurrent_histories_dont_erase_eachother() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let capacity = 7;
        let initial_entries = vec!["test 1", "test 2", "test 3", "test 4", "test 5"];
        let entries_a = vec!["A1", "A2", "A3"];
        let entries_b = vec!["B1", "B2", "B3"];
        let expected_entries = vec!["test 5", "B1", "B2", "B3", "A1", "A2", "A3"];

        {
            let mut writing_hist =
                SqliteBackedHistory::<()>::with_file(histfile.clone()).unwrap();

            initial_entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let mut hist_a = with_file_for_test(capacity, histfile.clone()).unwrap();

            {
                let mut hist_b = with_file_for_test(capacity, histfile.clone()).unwrap();

                entries_b.iter().for_each(|e| hist_b.append(e));

                // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            }
            entries_a.iter().for_each(|e| hist_a.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = with_file_for_test(capacity, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();
        assert_eq!(expected_entries, actual);

        tmp.close().unwrap();
    }

    #[test]
    fn concurrent_histories_are_threadsafe() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let num_threads = 16;
        let capacity = 2 * num_threads + 1;

        let initial_entries = (0..capacity).map(|i| format!("initial {i}"));

        {
            let mut writing_hist =
                with_file_for_test(capacity, histfile.clone()).unwrap();

            initial_entries.for_each(|e| writing_hist.append(&e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let threads = (0..num_threads)
            .map(|i| {
                let cap = capacity;
                let hfile = histfile.clone();
                std::thread::spawn(move || {
                    let mut hist = with_file_for_test(cap, hfile).unwrap();
                    hist.append(&format!("A{}", i));
                    hist.sync().unwrap();
                    hist.append(&format!("B{}", i));
                })
            })
            .collect::<Vec<_>>();

        for t in threads {
            t.join().unwrap();
        }

        let reading_hist = with_file_for_test(capacity, histfile).unwrap();

        let actual: Vec<_> = reading_hist.iter_chronologic().collect();

        assert!(
            actual.contains(&&format!("initial {}", capacity - 1)),
            "Overwrote entry from before threading test"
        );

        for i in 0..num_threads {
            assert!(actual.contains(&&format!("A{}", i)),);
            assert!(actual.contains(&&format!("B{}", i)),);
        }

        tmp.close().unwrap();
    }
}
