use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::io::{BufRead, BufWriter, Write};
use std::{collections::VecDeque, path::PathBuf};

/// Default size of the `History`
pub const HISTORY_SIZE: usize = 100;

/// Stateful history that allows up/down-arrow browsing with an internal cursor.
///
/// Can optionally be associated with a newline separated history file.
/// Similar to bash's behavior without HISTTIMEFORMAT.
/// (See https://www.gnu.org/software/bash/manual/html_node/Bash-History-Facilities.html)
/// All new changes within a certain History capacity will be written to disk when History is dropped.
#[derive(Debug)]
pub struct History {
    capacity: usize,
    entries: VecDeque<String>,
    cursor: usize, // If cursor == entries.len() outside history browsing
    file: Option<PathBuf>,
    len_on_disk: usize,  // Keep track what was previously written to disk
    truncate_file: bool, // as long as the file would not exceed capacity we can use appending writes
}

impl Default for History {
    fn default() -> Self {
        Self::new(HISTORY_SIZE)
    }
}

impl History {
    /// Creates a new in-memory history that remembers `<= capacity` elements
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
        }
    }

    /// Creates a new history with an associated history file.
    ///
    /// History file format: commands separated by new lines.
    /// If file exists file will be read otherwise empty file will be created.
    ///
    /// Side effects: creates all nested directories to the file
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
    /// Expects the `History` to be empty.
    ///
    /// Side effect: create/touch not yet existing file.
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
                let mut from_file: VecDeque<String> = reader.lines().map(|s| s.unwrap()).collect();
                let from_file = if from_file.len() > self.capacity {
                    from_file.split_off(from_file.len() - self.capacity)
                } else {
                    from_file
                };
                self.len_on_disk = from_file.len();
                self.entries = from_file;
                Ok(())
            }
        }
    }

    /// Writes unwritten history contents to disk.
    /// If file would exceed `capacity` truncates the oldest entries.
    fn flush(&mut self) -> std::io::Result<()> {
        if self.file.is_none() {
            return Ok(());
        }
        let file = if self.truncate_file {
            // Rewrite the whole file if we truncated the old output
            self.len_on_disk = 0;
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

    /// Access the underlying entries (exported for possible fancy access to underlying VecDeque)
    #[allow(dead_code)]
    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    /// Append an entry if non-empty and not repetition of the previous entry
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
                self.cursor = self.cursor.saturating_sub(1);
                self.len_on_disk = self.len_on_disk.saturating_sub(1);
                self.truncate_file = true;
            }
            // Keep the cursor meaning consistent if no call to `reset_cursor()` is done by the consumer
            if self.cursor == self.entries.len() {
                self.cursor += 1;
            }
            self.entries.push_back(entry);
        }
    }

    /// Reset the internal browsing cursor
    pub fn reset_cursor(&mut self) {
        self.cursor = self.entries.len()
    }

    /// Try to move back in history. Returns `None` if history is exhausted.
    pub fn go_back(&mut self) -> Option<&str> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(self.entries.get(self.cursor as usize).unwrap())
        } else {
            None
        }
    }

    /// Try to move forward in history. Returns `None` if history is exhausted (moving beyond most recent element).
    pub fn go_forward(&mut self) -> Option<&str> {
        if self.cursor < self.entries.len() {
            self.cursor += 1;
        }
        if self.cursor < self.entries.len() {
            Some(self.entries.get(self.cursor as usize).unwrap())
        } else {
            None
        }
    }

    /// Yields iterator to immutable references from the underlying data structure.
    /// Order: Oldest entries first.
    pub fn iter_chronologic(&self) -> std::collections::vec_deque::Iter<'_, String> {
        self.entries.iter()
    }

    /// Yields iterator to immutable references from the underlying data structure.
    /// Order: Most recent entries first.
    pub fn iter_recent(&self) -> std::iter::Rev<std::collections::vec_deque::Iter<'_, String>> {
        self.entries.iter().rev()
    }

    /// Helper to get items on zero based index starting at the most recent.
    pub fn get_nth_newest(&self, idx: usize) -> Option<&String> {
        self.entries.get(self.entries().len() - idx - 1)
    }

    /// Helper to get items on zero based index starting at the oldest entry.
    pub fn get_nth_oldest(&self, idx: usize) -> Option<&String> {
        self.entries.get(idx)
    }
}

impl Drop for History {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
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
}
