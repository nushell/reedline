use rusqlite::{named_params, params, Connection, MappedRows, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};

use super::{base::HistoryNavigationQuery, History};
use crate::core_editor::LineBuffer;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// This trait represents additional context to be added to a history (see [SqliteBackedHistory])
pub trait HistoryEntryContext: Serialize + DeserializeOwned + Default + Send {}

impl<T> HistoryEntryContext for T where T: Serialize + DeserializeOwned + Default + Send {}

/// A history that stores the values to an SQLite database.
/// In addition to storing the command, the history can store an additional arbitrary HistoryEntryContext,
/// to add information such as a timestamp, running directory, result...
pub struct SqliteBackedHistory<ContextType> {
    db: rusqlite::Connection,
    last_run_command_id: Option<i64>,
    last_run_command_context: Option<ContextType>,
    cursor: SqliteHistoryCursor,
}

// for arrow-navigation
struct SqliteHistoryCursor {
    id: i64,
    command: Option<String>,
    query: HistoryNavigationQuery,
}

/// Allow wrapping any history in Arc<Mutex<_>> so the creator of a Reedline can keep a reference to the history.
/// That way the library user can call history-specific methods ([`SqliteBackedHistory::update_last_command_context`])
/// The alternative would be that Reedline would have to be generic over the history type.
impl<H: History> History for Arc<Mutex<H>> {
    fn append(&mut self, entry: &str) {
        self.lock().expect("lock poisoned").append(entry)
    }

    fn iter_chronologic(&self) -> Box<(dyn DoubleEndedIterator<Item = std::string::String> + '_)> {
        let inner = self.lock().expect("lock poisoned");
        // TODO: performance :)
        Box::new(inner.iter_chronologic().collect::<Vec<_>>().into_iter())
    }

    fn back(&mut self) {
        self.lock().expect("lock poisoned").back()
    }

    fn forward(&mut self) {
        self.lock().expect("lock poisoned").forward()
    }

    fn string_at_cursor(&self) -> Option<String> {
        self.lock().expect("lock poisoned").string_at_cursor()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.lock()
            .expect("lock poisoned")
            .set_navigation(navigation)
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.lock().expect("lock poisoned").get_navigation()
    }

    fn query_entries(&self, search: &str) -> Vec<String> {
        self.lock().expect("lock poisoned").query_entries(search)
    }

    fn max_values(&self) -> usize {
        self.lock().expect("lock poisoned").max_values()
    }

    fn sync(&mut self) -> std::io::Result<()> {
        self.lock().expect("lock poisoned").sync()
    }

    fn reset_cursor(&mut self) {
        self.lock().expect("lock poisoned").reset_cursor()
    }
}

impl<ContextType: HistoryEntryContext> History for SqliteBackedHistory<ContextType> {
    /// Appends an entry if non-empty and not repetition of the previous entry.
    /// Resets the browsing cursor to the default state in front of the most recent entry.
    ///
    fn append(&mut self, entry: &str) {
        let ctx = ContextType::default();
        let ret: i64 = self
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
        self.last_run_command_id = Some(ret);
        self.last_run_command_context = Some(ctx);
        self.reset_cursor();
    }

    fn iter_chronologic(&self) -> Box<(dyn DoubleEndedIterator<Item = std::string::String> + '_)> {
        /*let mapper = |r: &Row| Ok((r.get(0)?, r.get(1)?));
        let fwd = inner
            .db
            .prepare("select id, command from history order by id asc").unwrap()
            .query_map(params![], mapper)
            .unwrap();
        let bwd = inner
            .db
            .prepare("select id, command from history order by id desc").unwrap()
            .query_map(params![], mapper)
            .unwrap();
        let de = SqliteDoubleEnded {
                fwd,
                bwd
            };*/
        // todo: read in chunks or dynamically (?)
        let fwd = self
            .db
            .prepare("select command from history order by id asc")
            .unwrap()
            .query_map(params![], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap();
        return Box::new(fwd.into_iter());
    }

    fn back(&mut self) {
        self.navigate_in_direction(true)
        // self.cursor.id
    }

    fn forward(&mut self) {
        self.navigate_in_direction(false)
    }

    fn string_at_cursor(&self) -> Option<String> {
        self.cursor.command.clone()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.cursor.query = navigation;
        self.reset_cursor();
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.cursor.query.clone()
    }

    fn query_entries(&self, search: &str) -> Vec<String> {
        self.iter_chronologic()
            .rev()
            .filter(|entry| entry.contains(search))
            .collect::<Vec<String>>()
    }

    fn max_values(&self) -> usize {
        self.last_run_command_id.unwrap_or(0) as usize
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn sync(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        // if no command run yet, fetch last id from db
        if self.last_run_command_id == None {
            self.last_run_command_id = self
                .db
                .prepare("select coalesce(max(id), 0) from history")
                .unwrap()
                .query_row(params![], |e| e.get(0))
                .optional()
                .unwrap();
        }
        self.cursor.id = self.last_run_command_id.unwrap_or(0) + 1;
        self.cursor.command = None;
    }
}
fn map_sqlite_err(err: rusqlite::Error) -> std::io::Error {
    // todo: better error mapping
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

impl<ContextType: HistoryEntryContext> SqliteBackedHistory<ContextType> {
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
    /// initialize a new database / migrate an existing one
    fn from_connection(db: Connection) -> std::io::Result<Self> {
        // https://phiresky.github.io/blog/2020/sqlite-performance-tuning/
        db.pragma_update(None, "journal_mode", "wal")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "synchronous", "normal")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "mmap_size", "1000000000")
            .map_err(map_sqlite_err)?;
        db.pragma_update(None, "foreign_keys", "on")
            .map_err(map_sqlite_err)?;
        db.execute(
            "
        create table if not exists history (
            id integer primary key autoincrement,
            command text not null,
            context text not null
        ) strict;
        ",
            params![],
        )
        .map_err(map_sqlite_err)?;
        let mut hist = SqliteBackedHistory {
            db,
            last_run_command_id: None,
            last_run_command_context: None,
            cursor: SqliteHistoryCursor {
                id: 0,
                command: None,
                query: HistoryNavigationQuery::Normal(LineBuffer::default()),
            },
        };
        hist.reset_cursor();
        Ok(hist)
    }

    // todo: better error type (which one?)
    /// updates the context stored for the last ran command
    pub fn update_last_command_context<F>(&mut self, callback: F) -> Result<(), String>
    where
        F: FnOnce(ContextType) -> ContextType,
    {
        if let (Some(id), Some(ctx)) = (
            self.last_run_command_id,
            self.last_run_command_context.take(),
        ) {
            let mapped_ctx = callback(ctx);
            self.db
                .execute(
                    "update history set context = :context where id = :id",
                    named_params! {
                        ":context": serde_json::to_string(&mapped_ctx).map_err(|e| format!("{e}"))?,
                        ":id": id
                    },
                )
                .map_err(|e| format!("{e}"))?;
            self.last_run_command_context.replace(mapped_ctx);
            Ok(())
        } else {
            Err(format!("No command has been executed yet"))
        }
    }

    fn navigate_in_direction(&mut self, backward: bool) {
        let like_str = match &self.cursor.query {
            HistoryNavigationQuery::Normal(_) => format!("%"),
            HistoryNavigationQuery::PrefixSearch(prefix) => format!("{prefix}%"),
            HistoryNavigationQuery::SubstringSearch(cont) => format!("%{cont}%"),
        };
        let query = if backward {
            "select id, command from history where id < :id and command like :like and command != :prev_result order by id desc limit 1"
        } else {
            "select id, command from history where id > :id and command like :like and command != :prev_result order by id asc limit 1"
        };
        let next_id: Option<(i64, String)> = self
            .db
            .prepare(query)
            .unwrap()
            .query_row(
                named_params! {
                    ":id": self.cursor.id,
                    ":like": like_str,
                    ":prev_result": self.cursor.command.clone().unwrap_or(String::new())
                },
                |e| Ok((e.get(0)?, e.get(1)?)),
            )
            .optional()
            .unwrap();
        if let Some((next_id, next_command)) = next_id {
            self.cursor.id = next_id;
            self.cursor.command = Some(next_command);
        } else {
            if !backward {
                // forward search resets to none, backwards search doesn't
                self.cursor.command = None;
            }
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
    fn with_file_for_test(
        capacity: i32,
        file: PathBuf,
    ) -> std::io::Result<SqliteBackedHistory<()>> {
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
    fn concurrent_histories_dont_erase_eachother() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join(".history");

        let capacity = 7;
        let initial_entries = vec!["test 1", "test 2", "test 3", "test 4", "test 5"];
        let entries_a = vec!["A1", "A2", "A3"];
        let entries_b = vec!["B1", "B2", "B3"];
        let expected_entries = vec![
            "test 1", "test 2", "test 3", "test 4", "test 5", "B1", "B2", "B3", "A1", "A2", "A3",
        ];

        {
            let mut writing_hist = SqliteBackedHistory::<()>::with_file(histfile.clone()).unwrap();

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
            let mut writing_hist = with_file_for_test(capacity, histfile.clone()).unwrap();

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
