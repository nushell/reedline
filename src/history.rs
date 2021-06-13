use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::io::{BufRead, BufWriter, Write};
use std::{collections::VecDeque, path::PathBuf};

/// Default size of the [`History`] used when calling [`History::default()`]
pub const HISTORY_SIZE: usize = 1000;

/// Stateful history that allows up/down-arrow browsing with an internal cursor.
///
/// ```
/// use reedline::History;
/// // Create a history with a capacity of 10 entries
/// let mut hist = History::new(10);
///
/// // Append entries...
/// hist.append(String::from("test command"));
/// // ... and browse through the history with `Option` based commands
/// assert_eq!(hist.go_back(), Some("test command"));
/// assert_eq!(hist.go_back(), None);
///
/// // If the number of entries exceeds `capacity` the oldest entry is dropped
/// for i in (0..10) {
///    hist.append(format!("{}", i));
/// }
/// assert_eq!(
///    hist.iter_chronologic().cloned().collect::<Vec<String>>(),
///    (0..10).map(|i| format!("{}", i)).collect::<Vec<_>>()
/// );
/// ```
///
/// Can optionally be associated with a newline separated history file using the [`History::with_file()`] constructor.
/// Similar to bash's behavior without HISTTIMEFORMAT.
/// (See <https://www.gnu.org/software/bash/manual/html_node/Bash-History-Facilities.html>)
/// If the history is associated to a file all new changes within a given history capacity will be written to disk when History is dropped.
#[derive(Debug)]
pub struct History {
    capacity: usize,
    entries: VecDeque<String>,
    cursor: usize, // If cursor == entries.len() outside history browsing
    file: Option<PathBuf>,
    len_on_disk: usize,  // Keep track what was previously written to disk
    truncate_file: bool, // as long as the file would not exceed capacity we can use appending writes

    /// The prefix to search the history in a stateful manner using [`History::go_forward_with_prefix`] and [`History::go_back_with_prefix`]
    pub history_prefix: Option<String>,
}

impl Default for History {
    /// Creates an in-memory [`History`] with a maximal capacity of [`HISTORY_SIZE`].
    ///
    /// To create a [`History`] that is synchronized with a file use [`History::with_file()`]
    fn default() -> Self {
        Self::new(HISTORY_SIZE)
    }
}

impl History {
    /// Creates a new in-memory history that remembers `n <= capacity` elements
    ///
    /// ```
    /// use reedline::History;
    /// let mut hist = History::new(10);
    /// assert_eq!(hist.go_back(), None);
    /// ```
    pub fn new(capacity: usize) -> Self {
        if capacity == usize::MAX {
            panic!("History capacity too large to be addressed safely");
        }
        History {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            cursor: 0,
            file: None,
            len_on_disk: 0,
            truncate_file: false,
            history_prefix: None,
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
    /// ```
    /// use tempfile::NamedTempFile;
    /// # use std::io::Write;
    /// # use reedline::History;
    ///
    /// let mut test_file = NamedTempFile::new().unwrap();
    /// test_file.write("test\ntext\nmore test text\n".as_bytes()).unwrap();
    ///
    /// let mut hist = History::with_file(10, test_file.path().to_owned()).unwrap();
    /// assert_eq!(hist.go_back(), Some("more test text"));
    /// assert_eq!(hist.iter_recent().count(), 3);
    /// ```
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

    /// Writes unwritten history contents to disk.
    ///
    /// If file would exceed `capacity` truncates the oldest entries.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if self.file.is_none() {
            return Ok(());
        }
        let file = if self.truncate_file {
            // Rewrite the whole file if we truncated the old output
            self.len_on_disk = 0;
            // TODO: make this file race safe if multiple instances are used.
            OpenOptions::new()
                .write(true)
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

    /// Access the underlying entries (exported for possible fancy access to underlying `VecDeque`)
    fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    /// Appends an entry if non-empty and not repetition of the previous entry.
    /// Resets the browsing cursor to the default state in front of the most recent entry.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("test"));
    /// hist.append(String::from(""));
    /// hist.append(String::from("repeat"));
    /// hist.append(String::from("repeat"));
    /// assert_eq!(hist.go_back(), Some("repeat"));
    /// assert_eq!(hist.go_back(), Some("test"));
    /// assert_eq!(hist.go_back(), None);
    /// ```
    pub fn append(&mut self, entry: String) {
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

    /// Reset the internal browsing cursor
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("old"));
    /// hist.append(String::from("most recent"));
    /// let _ = hist.go_back();
    /// let _ = hist.go_back();
    /// hist.reset_cursor();
    /// assert_eq!(hist.go_back(), Some("most recent"));
    /// ```
    pub fn reset_cursor(&mut self) {
        self.cursor = self.entries.len()
    }

    /// Try to move back in history.
    /// Returns [`None`] if history is exhausted.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("test"));
    /// assert_eq!(hist.go_back(), Some("test"));
    /// assert_eq!(hist.go_back(), None);
    /// ```
    pub fn go_back(&mut self) -> Option<&str> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(&self.entries[self.cursor])
        } else {
            None
        }
    }

    /// Try to search back in the history with the prefix stored in [`History::history_prefix`].
    /// Returns [`None`] if history is exhausted.
    /// Skips identical matches.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("find me as well"));
    /// hist.append(String::from("find me once"));
    /// hist.append(String::from("test"));
    /// hist.append(String::from("find me once"));
    /// hist.history_prefix = None;
    /// assert_eq!(hist.go_back_with_prefix(), None);
    /// hist.history_prefix = Some("find".to_string());
    /// assert_eq!(hist.go_back_with_prefix(), Some("find me once"));
    /// assert_eq!(hist.go_back_with_prefix(), Some("find me as well"));
    /// assert_eq!(hist.go_back_with_prefix(), None);
    /// ```
    pub fn go_back_with_prefix(&mut self) -> Option<&str> {
        if let Some(prefix) = &self.history_prefix {
            let old_match = self
                .entries
                .get(self.cursor)
                .filter(|entry| entry.starts_with(prefix));
            while self.cursor > 0 {
                self.cursor -= 1;
                let entry = &self.entries[self.cursor];
                if entry.starts_with(prefix) {
                    if old_match == Some(entry) {
                        continue;
                    }
                    return Some(entry);
                }
            }
        }

        None
    }

    /// Try to move forward in history.
    /// Returns [`None`] if history is exhausted (moving beyond most recent element).
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("old"));
    /// hist.append(String::from("test"));
    /// hist.append(String::from("new"));
    /// // Walk back
    /// assert_eq!(hist.go_back(), Some("new"));
    /// assert_eq!(hist.go_back(), Some("test"));
    /// assert_eq!(hist.go_back(), Some("old"));
    /// // Walk forward
    /// assert_eq!(hist.go_forward(), Some("test"));
    /// assert_eq!(hist.go_forward(), Some("new"));
    /// assert_eq!(hist.go_forward(), None);
    /// ```
    pub fn go_forward(&mut self) -> Option<&str> {
        if self.cursor < self.entries.len() {
            self.cursor += 1;
        }
        if self.cursor < self.entries.len() {
            Some(&self.entries[self.cursor])
        } else {
            None
        }
    }

    /// Try to search forward in the history with the prefix stored in [`History::history_prefix`].
    /// Returns [`History::history_prefix`] if history is exhausted.
    /// Skips identical matches.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("find me as well"));
    /// hist.append(String::from("find me once"));
    /// hist.append(String::from("test"));
    /// hist.append(String::from("find me once"));
    /// hist.append(String::from("test2"));
    /// hist.history_prefix = Some("find".to_string());
    /// // walk back
    /// assert_eq!(hist.go_back_with_prefix(), Some("find me once"));
    /// assert_eq!(hist.go_back_with_prefix(), Some("find me as well"));
    /// // walk forward
    /// assert_eq!(hist.go_forward_with_prefix(), Some("find me once"));
    /// assert_eq!(hist.go_forward_with_prefix(), Some("find"));
    /// ```
    pub fn go_forward_with_prefix(&mut self) -> Option<&str> {
        if let Some(prefix) = &self.history_prefix {
            let old_match = self
                .entries
                .get(self.cursor)
                .filter(|entry| entry.starts_with(prefix));
            while self.cursor < self.entries.len() {
                self.cursor += 1;

                if self.cursor < self.entries.len() {
                    let entry = &self.entries[self.cursor];
                    if entry.starts_with(prefix) {
                        if old_match == Some(entry) {
                            continue;
                        }
                        return Some(entry);
                    }
                }
            }
            Some(prefix)
        } else {
            None
        }
    }

    /// Yields iterator to immutable references from the underlying data structure.
    ///
    /// **Order:** Oldest entries first.
    pub fn iter_chronologic(
        &self,
    ) -> impl Iterator<Item = &String> + DoubleEndedIterator + ExactSizeIterator + '_ {
        self.entries.iter()
    }

    /// Yields iterator to immutable references from the underlying data structure.
    ///
    /// **Order:** Most recent entries first.
    pub fn iter_recent(
        &self,
    ) -> impl Iterator<Item = &String> + DoubleEndedIterator + ExactSizeIterator + '_ {
        self.entries.iter().rev()
    }

    /// Helper to get items on zero based index starting at the most recent entry.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("old"));
    /// hist.append(String::from("test"));
    /// hist.append(String::from("new"));
    ///
    /// assert_eq!(hist.get_nth_newest(0), Some(&"new".to_string()));
    /// assert_eq!(hist.get_nth_newest(1), Some(&"test".to_string()));
    /// ```
    pub fn get_nth_newest(&self, idx: usize) -> Option<&String> {
        self.entries.get(self.entries().len() - idx - 1)
    }

    /// Helper to get items on zero based index starting at the oldest entry.
    ///
    /// ```
    /// # use reedline::History;
    /// let mut hist = History::default();
    /// hist.append(String::from("old"));
    /// hist.append(String::from("test"));
    /// hist.append(String::from("new"));
    ///
    /// assert_eq!(hist.get_nth_oldest(0), Some(&"old".to_string()));
    /// assert_eq!(hist.get_nth_oldest(1), Some(&"test".to_string()));
    /// ```
    pub fn get_nth_oldest(&self, idx: usize) -> Option<&String> {
        self.entries.get(idx)
    }
}

impl Drop for History {
    /// On drop the content of the [`History`] will be written to the file if specified via [`History::with_file()`].
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufRead;

    use super::History;
    #[test]
    fn navigates_safely() {
        let mut hist = History::default();
        hist.append("test".to_string());
        assert_eq!(hist.go_forward(), None); // On empty line nothing to move forward to
        assert_eq!(hist.go_back().unwrap(), "test"); // Back to the entry
        assert_eq!(hist.go_back(), None); // Nothing to move back to
        assert_eq!(hist.go_forward(), None); // Forward out of history to editing line
    }
    #[test]
    fn appends_only_unique() {
        let mut hist = History::default();
        hist.append("unique_old".to_string());
        hist.append("test".to_string());
        hist.append("test".to_string());
        hist.append("unique".to_string());
        assert_eq!(hist.entries().len(), 3);
        assert_eq!(hist.go_back().unwrap(), "unique");
        assert_eq!(hist.go_back().unwrap(), "test");
        assert_eq!(hist.go_back().unwrap(), "unique_old");
        assert_eq!(hist.go_back(), None);
    }
    #[test]
    fn appends_no_empties() {
        let mut hist = History::default();
        hist.append("".to_string());
        assert_eq!(hist.entries().len(), 0);
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
            let mut hist = History::with_file(1000, histfile.clone()).unwrap();

            entries.iter().for_each(|e| hist.append(e.to_string()));

            // As `hist` goes out of scope and get's dropped, its contents are flushed to disk
        }

        let f = File::open(histfile).unwrap();

        let actual: Vec<String> = BufReader::new(f).lines().map(|x| x.unwrap()).collect();

        assert_eq!(entries, actual);

        tmp.close().unwrap();
    }
}
