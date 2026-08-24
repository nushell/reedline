use crate::{hinter::get_first_token, history::SearchQuery, Hinter, History};
use nu_ansi_term::{Color, Style};

/// A hinter that uses the completions or the history to show a hint to the user
pub struct DefaultHinter {
    style: Style,
    current_hint: String,
    min_chars: usize,
}

impl Hinter for DefaultHinter {
    fn handle(
        &mut self,
        line: &str,
        #[allow(unused_variables)] pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        self.current_hint = if line.chars().count() >= self.min_chars {
            history
                .search(SearchQuery::last_with_prefix(
                    line.to_string(),
                    history.session(),
                ))
                .unwrap_or_default()
                .first()
                .map_or_else(String::new, |entry| {
                    entry
                        .command_line
                        .get(line.len()..)
                        .unwrap_or_default()
                        .to_string()
                })
        } else {
            String::new()
        };

        if use_ansi_coloring && !self.current_hint.is_empty() {
            self.style.paint(&self.current_hint).to_string()
        } else {
            self.current_hint.clone()
        }
    }

    fn complete_hint(&self) -> String {
        self.current_hint.clone()
    }

    fn next_hint_token(&self) -> String {
        get_first_token(&self.current_hint)
    }
}

impl Default for DefaultHinter {
    fn default() -> Self {
        DefaultHinter {
            style: Style::new().fg(Color::LightGray),
            current_hint: String::new(),
            min_chars: 1,
        }
    }
}

impl DefaultHinter {
    /// A builder that sets the style applied to the hint as part of the buffer
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// A builder that sets the number of characters that have to be present to enable history hints
    #[must_use]
    pub fn with_min_chars(mut self, min_chars: usize) -> Self {
        self.min_chars = min_chars;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    use crate::{
        history::{FileBackedHistory, HistoryItem, HistoryItemId, HistorySessionId, SearchQuery},
        result::{ReedlineError, ReedlineErrorVariants},
        Result,
    };

    /// A backend whose every call fails, standing in for a locked or unreadable database.
    struct FailingHistory;

    fn fail<T>() -> Result<T> {
        Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
            "backend down",
        )))
    }

    impl History for FailingHistory {
        fn save(&mut self, _: HistoryItem) -> Result<HistoryItem> {
            fail()
        }
        fn load(&self, _: HistoryItemId) -> Result<HistoryItem> {
            fail()
        }
        fn count(&self, _: SearchQuery) -> Result<i64> {
            fail()
        }
        fn search(&self, _: SearchQuery) -> Result<Vec<HistoryItem>> {
            fail()
        }
        fn update(
            &mut self,
            _: HistoryItemId,
            _: &dyn Fn(HistoryItem) -> HistoryItem,
        ) -> Result<()> {
            fail()
        }
        fn clear(&mut self) -> Result<()> {
            fail()
        }
        fn delete(&mut self, _: HistoryItemId) -> Result<()> {
            fail()
        }
        fn sync(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn session(&self) -> Option<HistorySessionId> {
            None
        }
    }

    fn history_with(lines: &[&str]) -> FileBackedHistory {
        let mut history = FileBackedHistory::new(16).unwrap();
        for line in lines {
            history.save(HistoryItem::from_command_line(*line)).unwrap();
        }
        history
    }

    #[rstest]
    #[case::rest_of_latest_match("hello ", "world")]
    #[case::multibyte_prefix("café ", "latte")]
    #[case::exact_entry_leaves_nothing("hello world", "")]
    #[case::no_match("zzz", "")]
    fn hint_is_the_rest_of_the_latest_matching_entry(#[case] line: &str, #[case] hint: &str) {
        let history = history_with(&["café latte", "hello world"]);
        let mut hinter = DefaultHinter::default();
        assert_eq!(hinter.handle(line, line.len(), &history, false, ""), hint);
    }

    #[test]
    fn no_hint_below_min_chars() {
        let history = history_with(&["hello world"]);
        let mut hinter = DefaultHinter::default().with_min_chars(3);
        assert_eq!(hinter.handle("he", 2, &history, false, ""), "");
        assert_eq!(hinter.handle("hel", 3, &history, false, ""), "lo world");
    }

    #[test]
    fn accepting_exposes_the_whole_hint_and_its_first_token() {
        let history = history_with(&["git commit --amend"]);
        let mut hinter = DefaultHinter::default();
        hinter.handle("git ", 4, &history, false, "");
        assert_eq!(hinter.complete_hint(), "commit --amend");
        assert_eq!(hinter.next_hint_token(), "commit");
    }

    #[test]
    fn ansi_coloring_wraps_the_same_hint() {
        let history = history_with(&["hello world"]);
        let mut hinter = DefaultHinter::default();
        let painted = hinter.handle("hello ", 6, &history, true, "");
        assert_ne!(painted, "world");
        assert_eq!(strip_ansi_escapes::strip_str(&painted), "world");
    }

    #[test]
    fn a_failing_history_yields_no_hint_instead_of_a_panic() {
        let mut hinter = DefaultHinter::default();
        assert_eq!(hinter.handle("hello ", 6, &FailingHistory, false, ""), "");
    }
}
