use crate::{
    hinter::get_first_token,
    history::SearchQuery,
    result::{ReedlineError, ReedlineErrorVariants::HistoryFeatureUnsupported},
    Hinter, History,
};
use nu_ansi_term::{Color, Style};

/// A hinter that uses the completions or the history to show a hint to the user
///
/// Similar to `fish` autosuggestions
pub struct CwdAwareHinter {
    style: Style,
    current_hint: String,
    min_chars: usize,
}

impl Hinter for CwdAwareHinter {
    fn handle(
        &mut self,
        line: &str,
        #[allow(unused_variables)] pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
        cwd: &str,
    ) -> String {
        self.current_hint = if line.chars().count() >= self.min_chars {
            let with_cwd = history
                .search(SearchQuery::last_with_prefix_and_cwd(
                    line.to_string(),
                    cwd.to_string(),
                    history.session(),
                ))
                .or_else(|err| {
                    if let ReedlineError(HistoryFeatureUnsupported { .. }) = err {
                        history.search(SearchQuery::last_with_prefix(
                            line.to_string(),
                            history.session(),
                        ))
                    } else {
                        Err(err)
                    }
                })
                .unwrap_or_default();
            if let Some(first) = with_cwd.first() {
                first
                    .command_line
                    .get(line.len()..)
                    .unwrap_or_default()
                    .to_string()
            } else {
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
            }
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

    fn continues_text_before_cursor(&self) -> bool {
        true
    }
}

impl Default for CwdAwareHinter {
    fn default() -> Self {
        CwdAwareHinter {
            style: Style::new().fg(Color::LightGray),
            current_hint: String::new(),
            min_chars: 1,
        }
    }
}

impl CwdAwareHinter {
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
    use crate::history::{FileBackedHistory, HistoryItem};
    use rstest::rstest;

    fn file_history(lines: &[&str]) -> FileBackedHistory {
        let mut history = FileBackedHistory::new(16).unwrap();
        for line in lines {
            history.save(HistoryItem::from_command_line(*line)).unwrap();
        }
        history
    }

    // `FileBackedHistory` cannot filter by cwd, so these exercise the fallback
    // to the plain prefix search.
    #[rstest]
    #[case::rest_of_latest_match("hello ", "world")]
    #[case::multibyte_prefix("café ", "latte")]
    #[case::exact_entry_leaves_nothing("hello world", "")]
    #[case::no_match("zzz", "")]
    fn falls_back_to_prefix_search_without_cwd_support(#[case] line: &str, #[case] hint: &str) {
        let history = file_history(&["café latte", "hello world"]);
        let mut hinter = CwdAwareHinter::default();
        assert_eq!(
            hinter.handle(line, line.len(), &history, false, "/tmp"),
            hint
        );
    }

    #[test]
    fn no_hint_below_min_chars() {
        let history = file_history(&["éa latte", "hello world"]);
        let mut hinter = CwdAwareHinter::default().with_min_chars(3);
        assert_eq!(hinter.handle("hel", 3, &history, false, "/tmp"), "lo world");
        assert_eq!(hinter.complete_hint(), "lo world");

        // The hint of the longer line does not outlive it.
        assert_eq!(hinter.handle("he", 2, &history, false, "/tmp"), "");
        assert_eq!(hinter.complete_hint(), "");

        // Two chars in three bytes: the threshold counts chars.
        assert_eq!(hinter.handle("éa", 3, &history, false, "/tmp"), "");
        assert_eq!(hinter.complete_hint(), "");
    }

    #[test]
    fn colors_only_a_non_empty_hint() {
        let history = file_history(&["hello world"]);
        let style = Style::new().fg(Color::Red);
        let mut hinter = CwdAwareHinter::default().with_style(style);

        assert_eq!(
            hinter.handle("hello", 5, &history, true, "/tmp"),
            style.paint(" world").to_string()
        );
        assert_eq!(hinter.complete_hint(), " world");
        assert_eq!(hinter.handle("zzz", 3, &history, true, "/tmp"), "");
    }

    #[test]
    fn next_token_is_the_first_word_of_the_hint() {
        let history = file_history(&["git commit -m wip"]);
        let mut hinter = CwdAwareHinter::default();
        hinter.handle("git", 3, &history, false, "/tmp");
        assert_eq!(hinter.next_hint_token(), " commit");
    }

    #[cfg(feature = "_sqlite")]
    mod with_cwd {
        use super::*;
        use crate::history::SqliteBackedHistory;

        fn sqlite_history(entries: &[(&str, &str)]) -> SqliteBackedHistory {
            let mut history = SqliteBackedHistory::in_memory().unwrap();
            for (line, cwd) in entries {
                let mut item = HistoryItem::from_command_line(*line);
                item.cwd = Some(cwd.to_string());
                history.save(item).unwrap();
            }
            history
        }

        #[test]
        fn prefers_an_entry_from_the_current_directory() {
            let history = sqlite_history(&[("cargo test", "/repo"), ("cargo build", "/other")]);
            let mut hinter = CwdAwareHinter::default();
            assert_eq!(hinter.handle("cargo ", 6, &history, false, "/repo"), "test");
            assert_eq!(
                hinter.handle("cargo ", 6, &history, false, "/other"),
                "build"
            );
        }

        #[test]
        fn falls_back_to_other_directories() {
            let history = sqlite_history(&[("cargo test", "/repo")]);
            let mut hinter = CwdAwareHinter::default();
            assert_eq!(
                hinter.handle("cargo ", 6, &history, false, "/elsewhere"),
                "test"
            );
        }
    }
}
