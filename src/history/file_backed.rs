use std::{
    collections::{vec_deque::Iter, VecDeque},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
};

use super::{
    base::{HistoryAppender, HistoryNavigationQuery, HistoryView},
    History,
};

/// Default size of the [`FileBackedHistory`] used when calling [`FileBackedHistory::default()`]
pub const HISTORY_SIZE: usize = 1000;

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
    len_on_disk: usize,  // Keep track what was previously written to disk
    truncate_file: bool, // as long as the file would not exceed capacity we can use appending writes
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

impl History for FileBackedHistory {}

impl HistoryAppender for FileBackedHistory {
    /// Appends an entry if non-empty and not repetition of the previous entry.
    /// Resets the browsing cursor to the default state in front of the most recent entry.
    ///
    fn append(&mut self, entry: String) {
        // Don't append if the preceding value is identical or the string empty
        if self
            .entries
            .back()
            .map_or(true, |previous| previous != &entry)
            && !entry.is_empty()
        {
            if self.entries.len() == self.capacity {
                // History is "full", so we delete the oldest entry first,
                // before adding a new one.
                self.entries.pop_front();
                self.len_on_disk = self.len_on_disk.saturating_sub(1);
                self.truncate_file = true;
            }
            self.entries.push_back(entry);
        }
        self.reset_cursor()
    }

    fn iter_chronologic(&self) -> Iter<'_, String> {
        self.entries.iter()
    }
}

impl HistoryView for FileBackedHistory {
    fn back(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.back_with_criteria(&|entry| entry.starts_with(&prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.back_with_criteria(&|entry| entry.contains(&substring))
            }
        }
    }

    fn forward(&mut self) {
        match self.query.clone() {
            HistoryNavigationQuery::Normal => {
                if (self.cursor as isize) < self.entries.len() as isize - 1 {
                    self.cursor += 1;
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                self.forward_with_criteria(&|entry| entry.starts_with(&prefix))
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                self.forward_with_criteria(&|entry| entry.contains(&substring))
            }
        }
    }

    fn string_at_cursor(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let entry = self.entries[self.cursor].to_string();

        match self.query.clone() {
            HistoryNavigationQuery::Normal => Some(entry),
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                if entry.starts_with(&prefix) {
                    Some(entry)
                } else {
                    None
                }
            }
            HistoryNavigationQuery::SubstringSearch(substring) => {
                if substring.is_empty() {
                    return None;
                }
                if entry.contains(&substring) {
                    Some(entry)
                } else {
                    None
                }
            }
        }
    }

    fn set_navigation(&mut self, navigation: HistoryNavigationQuery) {
        self.query = navigation;
        self.reset_cursor();
    }

    fn get_navigation(&self) -> HistoryNavigationQuery {
        self.query.clone()
    }
}

impl FileBackedHistory {
    /// Creates a new in-memory history that remembers `n <= capacity` elements
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
            truncate_file: true,
            query: HistoryNavigationQuery::Normal,
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
        hist.load_file()?;
        Ok(hist)
    }

    /// Loads history from the associated newline separated file
    ///
    /// Expects the [`History`] to be empty.
    ///
    ///
    /// **Side effect:** creates not yet existing file.
    fn load_file(&mut self) -> std::io::Result<()> {
        let f = File::open(
            self.file
                .as_ref()
                .expect("History::load_file should only be called if a filename is set"),
        );
        assert!(
            self.entries.is_empty(),
            "History currently designed to load file once in the constructor"
        );
        match f {
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    File::create(self.file.as_ref().unwrap())?;
                    Ok(())
                }
                _ => Err(e),
            },
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut from_file: VecDeque<String> = reader.lines().map(Result::unwrap).collect();
                let from_file = if from_file.len() > self.capacity {
                    from_file.split_off(from_file.len() - self.capacity)
                } else {
                    from_file
                };
                self.len_on_disk = from_file.len();
                self.entries = from_file;
                self.reset_cursor();
                Ok(())
            }
        }
    }

    fn back_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        let mut cursor = self.cursor;
        let previous_match = self.string_at_cursor();

        while cursor > 0 {
            cursor -= 1;
            let entry = &self.entries[cursor];
            if criteria(entry) {
                if previous_match
                    // TODO Get rid of this clone
                    .clone()
                    .map_or(false, |value| &value == entry)
                {
                    continue;
                } else {
                    break;
                }
            }
        }

        self.cursor = cursor;
    }

    fn forward_with_criteria(&mut self, criteria: &dyn Fn(&str) -> bool) {
        let mut cursor = self.cursor;
        let previous_match = self.string_at_cursor();

        while cursor < self.entries.len() - 1 {
            cursor += 1;
            let entry = &self.entries[cursor];
            if criteria(entry) {
                // if entry.contains(&substring) {
                if previous_match
                    // TODO Get rid of this clone
                    .clone()
                    .map_or(false, |value| &value == entry)
                {
                    continue;
                } else {
                    break;
                }
            }
        }

        self.cursor = cursor;
    }

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    fn flush(&mut self) -> std::io::Result<()> {
        if self.file.is_none() {
            return Ok(());
        }
        let file = if self.truncate_file {
            // Rewrite the whole file if we truncated the old output
            self.len_on_disk = 0;
            // TODO: make this file race safe if multiple instances are used.
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(self.file.as_ref().unwrap())?
        } else {
            // If the file is not beyond capacity just append new stuff
            // (use the stored self.len_on_disk as offset)
            OpenOptions::new()
                .append(true)
                .open(self.file.as_ref().unwrap())?
        };
        let mut writer = BufWriter::new(file);
        for line in self.entries.range(self.len_on_disk..) {
            writer.write_all(line.as_bytes())?;
            writer.write_all("\n".as_bytes())?;
        }
        writer.flush()?;
        self.len_on_disk = self.entries.len();

        Ok(())
    }

    /// Reset the internal browsing cursor
    fn reset_cursor(&mut self) {
        if self.entries.is_empty() {
            self.cursor = 0
        } else {
            self.cursor = self.entries.len() - 1;
        }
    }
}

impl Drop for FileBackedHistory {
    /// On drop the content of the [`History`] will be written to the file if specified via [`FileBackedHistory::with_file()`].
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::io::BufRead;

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
        hist.append("command1".to_string());
        hist.append("command2".to_string());
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
        hist.append("command1".to_string());
        hist.append("command2".to_string());
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        hist.forward();
        assert_eq!(hist.string_at_cursor(), Some("command2".to_string()));
    }

    #[test]
    fn appends_only_unique() {
        let mut hist = FileBackedHistory::default();
        hist.append("unique_old".to_string());
        hist.append("test".to_string());
        hist.append("test".to_string());
        hist.append("unique".to_string());
        assert_eq!(hist.entries.len(), 3);
    }
    #[test]
    fn appends_no_empties() {
        let mut hist = FileBackedHistory::default();
        hist.append("".to_string());
        assert_eq!(hist.entries.len(), 0);
    }

    #[test]
    fn prefix_search_works() {
        let mut hist = FileBackedHistory::default();
        hist.append(String::from("find me as well"));
        hist.append(String::from("test"));
        hist.append(String::from("find me"));

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));

        assert_eq!(hist.string_at_cursor(), Some("find me".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }

    #[test]
    fn prefix_search_bottoms_out() {
        let mut hist = FileBackedHistory::default();
        hist.append(String::from("find me as well"));
        hist.append(String::from("test"));
        hist.append(String::from("find me"));

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));

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
    fn prefix_search_ignores_consecitive_equivalent_entries_going_backwards() {
        let mut hist = FileBackedHistory::default();
        hist.append(String::from("find me as well"));
        hist.append(String::from("find me once"));
        hist.append(String::from("test"));
        hist.append(String::from("find me once"));

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));

        assert_eq!(hist.string_at_cursor(), Some("find me once".to_string()));
        hist.back();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }

    #[test]
    fn prefix_search_ignores_consecitive_equivalent_entries_going_forwards() {
        let mut hist = FileBackedHistory::default();
        hist.append(String::from("find me once"));
        hist.append(String::from("test"));
        hist.append(String::from("find me once"));
        hist.append(String::from("find me as well"));

        hist.set_navigation(HistoryNavigationQuery::PrefixSearch("find".to_string()));
        hist.cursor = 0;

        assert_eq!(hist.string_at_cursor(), Some("find me once".to_string()));
        hist.forward();
        assert_eq!(hist.string_at_cursor(), Some("find me as well".to_string()));
    }

    #[test]
    fn substring_search_works() {
        let mut hist = FileBackedHistory::default();
        hist.append(String::from("substring"));
        hist.append(String::from("don't find me either"));
        hist.append(String::from("prefix substring"));
        hist.append(String::from("don't find me"));
        hist.append(String::from("prefix substring suffix"));

        hist.set_navigation(HistoryNavigationQuery::SubstringSearch(
            "substring".to_string(),
        ));

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
        hist.append(String::from("substring"));

        hist.set_navigation(HistoryNavigationQuery::SubstringSearch("".to_string()));

        assert_eq!(hist.string_at_cursor(), None);
    }

    #[test]
    fn writes_to_new_file() {
        use std::fs::File;
        use std::io::BufReader;
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let histfile = tmp.path().join("nested_path").join(".history");

        let entries = vec!["test", "text", "more test text"];

        {
            let mut hist = FileBackedHistory::with_file(1000, histfile.clone()).unwrap();

            entries.iter().for_each(|e| hist.append(e.to_string()));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let f = File::open(histfile).unwrap();

        let actual: Vec<String> = BufReader::new(f).lines().map(|x| x.unwrap()).collect();

        assert_eq!(entries, actual);

        tmp.close().unwrap();
    }
}
