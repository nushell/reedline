use std::{collections::VecDeque, ops::Index};

/// Default size of the `History`
const HISTORY_SIZE: usize = 100;

/// Stateful history that allows up/down-arrow browsing with an internal cursor
pub struct History {
    capacity: usize,
    entries: VecDeque<String>,
    cursor: isize, // -1 not browsing through history, >= 0 index into history
}

impl Default for History {
    fn default() -> Self {
        Self::new(HISTORY_SIZE)
    }
}

impl History {
    /// Creates an in-memory history that remembers `<= capacity` elements
    pub fn new(capacity: usize) -> Self {
        if capacity > isize::MAX as usize {
            panic!("History capacity too large to be addressed safely");
        }
        History {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            cursor: -1,
        }
    }

    /// Access the underlying entries (exported for possible fancy access to underlying VecDeque)
    #[allow(dead_code)]
    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    /// Append an entry if non-empty and not repetition of the previous entry
    pub fn append(&mut self, entry: String) {
        if self.entries.len() + 1 == self.capacity {
            // History is "full", so we delete the oldest entry first,
            // before adding a new one.
            self.entries.pop_back();
        }
        // Don't append if the preceding value is identical or the string empty
        if self
            .entries
            .front()
            .map_or(true, |previous| previous != &entry)
            && !entry.is_empty()
        {
            self.entries.push_front(entry);
        }
    }

    /// Reset the internal browsing cursor
    pub fn reset_cursor(&mut self) {
        self.cursor = -1
    }

    /// Try to move back in history. Returns `None` if history is exhausted.
    pub fn go_back(&mut self) -> Option<&str> {
        if self.cursor < (self.entries.len() as isize - 1) {
            self.cursor += 1;
            Some(self.entries.get(self.cursor as usize).unwrap())
        } else {
            None
        }
    }

    /// Try to move forward in history. Returns `None` if history is exhausted (moving beyond most recent element).
    pub fn go_forward(&mut self) -> Option<&str> {
        if self.cursor >= 0 {
            self.cursor -= 1;
        }
        if self.cursor >= 0 {
            Some(self.entries.get(self.cursor as usize).unwrap())
        } else {
            None
        }
    }

    /// Yields iterator to immutable references from the underlying data structure
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, String> {
        self.entries.iter()
    }
}

impl Index<usize> for History {
    type Output = String;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
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
