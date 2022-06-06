use std::ops::Deref;

use crate::{
    history::SearchQuery, menu_functions::parse_selection_char, Completer, History, Span,
    Suggestion,
};

const SELECTION_CHAR: char = '!';

// The HistoryCompleter is created just before updating the menu
// It pulls data from the object that contains access to the History
pub(crate) struct HistoryCompleter<'menu>(&'menu dyn History);

// Safe to implement Send since the Historycompleter should only be used when
// updating the menu and that must happen in the same thread
unsafe impl<'menu> Send for HistoryCompleter<'menu> {}

impl<'menu> Completer for HistoryCompleter<'menu> {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let parsed = parse_selection_char(line, SELECTION_CHAR);
        let values = self
            .0
            .search(SearchQuery::all_that_contain_rev(
                parsed.remainder.to_string(),
            ))
            .expect("todo: error handling");

        values
            .into_iter()
            .map(|value| self.create_suggestion(line, pos, value.command_line.deref()))
            .collect()
    }

    // TODO: Implement `fn partial_complete()`

    fn total_completions(&mut self, line: &str, _pos: usize) -> usize {
        let parsed = parse_selection_char(line, SELECTION_CHAR);
        let count = self
            .0
            .count(SearchQuery::all_that_contain_rev(
                parsed.remainder.to_string(),
            ))
            .expect("todo: error handling");
        count as usize
    }
}

impl<'menu> HistoryCompleter<'menu> {
    pub fn new(history: &'menu dyn History) -> Self {
        Self(history)
    }

    fn create_suggestion(&self, line: &str, pos: usize, value: &str) -> Suggestion {
        let span = Span {
            start: pos,
            end: pos + line.len(),
        };

        Suggestion {
            value: value.to_string(),
            description: None,
            extra: None,
            span,
            append_whitespace: false,
        }
    }
}
