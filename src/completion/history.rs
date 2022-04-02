use std::ops::Deref;

use crate::{menu_functions::parse_selection_char, Completer, History, Span, Suggestion};

const SELECTION_CHAR: char = '!';

// The HistoryCompleter is created just before updating the menu
// It pulls data from the object that contains access to the History
pub(crate) struct HistoryCompleter<'menu>(&'menu dyn History);

// Safe to implement Send since the Historycompleter should only be used when
// updating the menu and that must happen in the same thread
unsafe impl<'menu> Send for HistoryCompleter<'menu> {}

impl<'menu> Completer for HistoryCompleter<'menu> {
    fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let parsed = parse_selection_char(line, SELECTION_CHAR);
        let values = self.0.query_entries(parsed.remainder);

        values
            .into_iter()
            .map(|value| self.create_suggestion(line, pos, value.deref()))
            .collect()
    }

    fn partial_complete(
        &self,
        line: &str,
        pos: usize,
        start: usize,
        offset: usize,
    ) -> Vec<Suggestion> {
        self.0
            .iter_chronologic()
            .rev()
            .skip(start)
            .take(offset)
            .map(|value| self.create_suggestion(line, pos, value.deref()))
            .collect()
    }

    fn total_completions(&self, _line: &str, _pos: usize) -> usize {
        self.0.max_values()
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
        }
    }
}
