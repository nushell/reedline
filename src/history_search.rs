use crate::history::History;

/// Implements a reverse search through the history.
/// Stores a search string for incremental search and remembers the last result
/// to allow browsing through ambiguous search results.
#[derive(Clone)]
pub struct BasicSearch {
    pub result: Option<(usize, usize)>,
    pub search_string: String,
}

pub enum BasicSearchCommand {
    InsertChar(char),
    Backspace,
    Next,
}

impl BasicSearch {
    // TODO: do we want to initialize the string if we don't compute an immediate result?
    pub fn new(search_string: String) -> Self {
        Self {
            result: None,
            search_string,
        }
    }

    /// Perform a step of incremental search.
    /// Either change the search string or go one result back in history.
    ///
    /// Sets [`BasicSearch.result`] `Option<(idx, offset)>` with:
    ///
    /// `idx`: 0-based index starting at the newest history entries.
    /// `offset`: location in the text where the match was found.
    pub fn step(&mut self, command: BasicSearchCommand, history: &History) {
        let mut start = self
            .result
            .map(|(history_index, _)| history_index)
            .unwrap_or(0);

        match command {
            BasicSearchCommand::InsertChar(c) => {
                self.search_string.push(c);
            }
            BasicSearchCommand::Backspace => {
                self.search_string.pop(); // TODO: Unicode grapheme?
            }
            BasicSearchCommand::Next => {
                start += 1;
            }
        }

        if self.search_string.is_empty() {
            self.result = None;
        } else {
            self.result = history
                .iter_recent()
                .enumerate()
                .skip(start)
                .filter_map(|(history_index, s)| {
                    s.match_indices(&self.search_string)
                        .next()
                        .map(|(offset, _)| (history_index, offset))
                })
                .next();
        }
    }
}
