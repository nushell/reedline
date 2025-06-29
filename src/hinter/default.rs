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
                .expect("todo: error handling")
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
