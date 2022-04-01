use super::{base::HistoryNavigationQuery, History};
use crate::core_editor::LineBuffer;
use std::{
    collections::{vec_deque::Iter, VecDeque},
    fs::OpenOptions,
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

/// Default size of the [`FileBackedHistory`] used when calling [`FileBackedHistory::default()`]
pub const HISTORY_SIZE: usize = 1000;
pub const NEWLINE_ESCAPE: &str = "<\\n>";

/// Stateful history that allows up/down-arrow browsing with an internal cursor.
///
/// Can optionally be associated with a newline separated history file using the [`FileBackedHistory::with_file()`] constructor.
/// Similar to bash's behavior without HISTTIMEFORMAT.
/// (See <https://www.gnu.org/software/bash/manual/html_node/Bash-History-Facilities.html>)
/// If the history is associated to a file all new changes within a given history capacity will be written to disk when History is dropped.
#[derive(Debug)]
pub struct FileBackedHistory {
    capacity: usize,
    entries: VecDeque<String>,
    cursor: usize, // If cursor == entries.len() outside history browsing
    file: Option<PathBuf>,
    len_on_disk: usize, // Keep track what was previously written to disk
    query: HistoryNavigationQuery,
}

impl Default for FileBackedHistory {
    /// Creates an in-memory [`History`] with a maximal capacity of [`HISTORY_SIZE`].
    ///
    /// To create a [`History`] that is synchronized with a file use [`FileBackedHistory::with_file()`]
    fn default() -> Self {
        Self::new(HISTORY_SIZE)
    }
}

fn encode_entry(s: &str) -> String {
    s.replace('\n', NEWLINE_ESCAPE)
}

fn decode_entry(s: &str) -> String {
    s.replace(NEWLINE_ESCAPE, "\n")
}

impl History for FileBackedHistory {
    /// Appends an entry if non-empty and not repetition of the previous entry.
    /// Resets the browsing cursor to the default state in front of the most recent entry.
    ///
    fn append(&mut self, entry: &str) {
        // Don't append if the preceding value is identical or the string empty
        if self
            .entries
            .back()
            .map_or(true, |previous| previous != entry)
            && !entry.is_empty()
        {
            if self.entries.len() == self.capacity {
                // History is "full", so we delete the oldest entry first,
                // before adding a new one.
                self.entries.pop_front();
                self.len_on_disk = self.len_on_disk.saturating_sub(1);
            }
            self.entries.push_back(entry.to_string());
        }
        self.reset_cursor();
    }

    fn iter_chronologic(&self) -> Box<(dyn DoubleEndedIterator<Item = String> + '_)> {
        Box::new(self.entries.iter().map(|e| e.to_string()))
    }

    fn back(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.back_with_criteria(&|entry| entry.starts_with(&prefix));
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.back_with_criteria(&|entry| entry.contains(&substring));
            }
        }
    }

    fn forward(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal(_) => {
                if self.cursor < self.entries.len() {
                    self.cursor += 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.forward_with_criteria(&|entry| entry.starts_with(&prefix));
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.forward_with_criteria(&|entry| entry.contains(&substring));
            }
        }
    }

    fn string_at_cursor(&self) -> Option<String> {
        self.entries.get(self.cursor).cloned()
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.query = navigation;
        self.reset_cursor();
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.query.clone()
    }

    fn query_entries(&self, search: &str) -> Vec<String> {
        self.iter_chronologic()
            .rev()
            .filter(|entry| entry.contains(search))
            .collect::<Vec<String>>()
    }

    fn max_values(&self) -> usize {
        self.entries.len()
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn sync(&mut self) -> std::io::Result<()> {
        if let Some(fname) = &self.file {
            // The unwritten entries
            let own_entries = self.entries.range(self.len_on_disk..);

            let mut f_lock = fd_lock::RwLock::new(
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .read(true)
                    .open(fname)?,
            );
            let mut writer_guard = f_lock.write()?;
            let (mut foreign_entries, truncate) = {
                let reader = BufReader::new(writer_guard.deref());
                let mut from_file = reader
                    .lines()
                    .map(|o| o.map(|i| decode_entry(&i)))
                    .collect::<Result<VecDeque<_>, _>>()?;
                if from_file.len() + own_entries.len() > self.capacity {
                    (
                        from_file.split_off(from_file.len() - (self.capacity - own_entries.len())),
                        true,
                    )
                } else {
                    (from_file, false)
                }
            };

            {
                let mut writer = BufWriter::new(writer_guard.deref_mut());
                if truncate {
                    writer.seek(SeekFrom::Start(0))?;

                    for line in &foreign_entries {
                        writer.write_all(encode_entry(line).as_bytes())?;
                        writer.write_all("\n".as_bytes())?;
                    }
                } else {
                    writer.seek(SeekFrom::End(0))?;
                }
                for line in own_entries {
                    writer.write_all(encode_entry(line).as_bytes())?;
                    writer.write_all("\n".as_bytes())?;
                }
                writer.flush()?;
            }
            if truncate {
                let file = writer_guard.deref_mut();
                let file_len = file.stream_position()?;
                file.set_len(file_len)?;
            }

            let own_entries = self.entries.drain(self.len_on_disk..);
            foreign_entries.extend(own_entries);
            self.entries = foreign_entries;

            self.len_on_disk = self.entries.len();
        }

        self.reset_cursor();

        Ok(())
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        self.cursor = self.entries.len();
    }
}

impl FileBackedHistory {
    /// Creates a new in-memory history that remembers `n <= capacity` elements
    ///
    /// # Panics
    ///
    /// If `capacity == usize::MAX`
    pub fn new(capacity: usize) -> Self {
        if capacity == usize::MAX {
            panic!("History capacity too large to be addressed safely");
        }
        FileBackedHistory {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            cursor: 0,
            file: None,
            len_on_disk: 0,
            query: HistoryNavigationQuery::Normal(LineBuffer::default()),
        }
    }

    /// Creates a new history with an associated history file.
    ///
    /// History file format: commands separated by new lines.
    /// If file exists file will be read otherwise empty file will be created.
    ///
    ///
    /// **Side effects:** creates all nested directories to the file
    ///
    pub fn with_file(capacity: usize, file: PathBuf) -> std::io::Result<Self> {
        let mut hist = Self::new(capacity);
        if let Some(base_dir) = file.parent() {
            std::fs::create_dir_all(base_dir)?;
        }
        hist.file = Some(file);
        hist.sync()?;
        Ok(hist)
    }

    fn back_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        if !self.entries.is_empty() {
            let previous_match = self.entries.get(self.cursor);
            if let Some((next_cursor, _)) = self
                .entries
                .iter()
                .take(self.cursor)
                .enumerate()
                .rev()
                .find(|(_, entry)| criteria(entry) && previous_match != Some(entry))
            {
                // set to entry
                self.cursor = next_cursor;
            }
        }
    }

    fn forward_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        let previous_match = self.entries.get(self.cursor);
        if let Some((next_cursor, _)) = self
            .entries
            .iter()
            .enumerate()
            .skip(self.cursor + 1)
            .find(|(_, entry)| criteria(entry) && previous_match != Some(entry))
        {
            // set to entry
            self.cursor = next_cursor;
        } else {
            self.reset_cursor();
        }
    }
}

impl Drop for FileBackedHistory {
    /// On drop the content of the [`History`] will be written to the file if specified via [`FileBackedHistory::with_file()`].
    fn drop(&mut self) {
        let _res = self.sync();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn accessing_empty_history_returns_nothing() {
        let hist = FileBackedHistory::default();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_forward_in_empty_history_does_not_error_out() {
        let mut hist = FileBackedHistory::default();
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_backwards_in_empty_history_does_not_error_out() {
        let mut hist = FileBackedHistory::default();
        hist.back();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn going_backwards_bottoms_out() {
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
        hist.append("command1");
        hist.append("command2");
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn appends_only_unique() {
        let mut hist = FileBackedHistory::default();
        hist.append("unique_old");
        hist.append("test");
        hist.append("test");
        hist.append("unique");
        assert_eq!(hist.entries.len(), 3);
    }
    #[test]
    fn appends_no_empties() {
        let mut hist = FileBackedHistory::default();
        hist.append("");
        assert_eq!(hist.entries.len(), 0);
    }

    #[test]
    fn prefix_search_works() {
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
        let mut hist = FileBackedHistory::default();
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
            let mut hist = FileBackedHistory::with_file(5, histfile.clone()).unwrap();

            entries.iter().for_each(|e| hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = FileBackedHistory::with_file(5, histfile).unwrap();

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
            let mut writing_hist = FileBackedHistory::with_file(5, histfile.clone()).unwrap();

            entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = FileBackedHistory::with_file(5, histfile).unwrap();

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
                FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            initial_entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let mut appending_hist =
                FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            appending_entries
                .iter()
                .for_each(|e| appending_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            let actual: Vec<_> = appending_hist.iter_chronologic().collect();
            assert_eq!(expected_appended_entries, actual);
        }

        {
            let mut truncating_hist =
                FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            truncating_entries
                .iter()
                .for_each(|e| truncating_hist.append(e));

            let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = FileBackedHistory::with_file(capacity, histfile).unwrap();

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
            let mut writing_hist = FileBackedHistory::with_file(10, histfile.clone()).unwrap();

            overly_large_previous_entries
                .iter()
                .for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let truncating_hist = FileBackedHistory::with_file(5, histfile.clone()).unwrap();

            let actual: Vec<_> = truncating_hist.iter_chronologic().collect();
            assert_eq!(expected_truncated_entries, actual);
            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = FileBackedHistory::with_file(5, histfile).unwrap();

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
                FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            initial_entries.iter().for_each(|e| writing_hist.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        {
            let mut hist_a = FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            {
                let mut hist_b = FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

                entries_b.iter().for_each(|e| hist_b.append(e));

                // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
            }
            entries_a.iter().for_each(|e| hist_a.append(e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let reading_hist = FileBackedHistory::with_file(capacity, histfile).unwrap();

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
                FileBackedHistory::with_file(capacity, histfile.clone()).unwrap();

            initial_entries.for_each(|e| writing_hist.append(&e));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let threads = (0..num_threads)
            .map(|i| {
                let cap = capacity;
                let hfile = histfile.clone();
                std::thread::spawn(move || {
                    let mut hist = FileBackedHistory::with_file(cap, hfile).unwrap();
                    hist.append(&format!("A{}", i));
                    hist.sync().unwrap();
                    hist.append(&format!("B{}", i));
                })
            })
            .collect::<Vec<_>>();

        for t in threads {
            t.join().unwrap();
        }

        let reading_hist = FileBackedHistory::with_file(capacity, histfile).unwrap();

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
