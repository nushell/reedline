use {
    crate::History,
    nu_ansi_term::{Color, Style},
};

/// A trait that's responsible for returning the hint for the current line and position
/// Hints are often shown in-line as part of the buffer, showing the user text they can accept or ignore
pub trait Hinter: Send {
    /// Handle the hinting duty by using the line, position, and current history
    ///
    /// Returns the formatted output to show the user
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
    ) -> String;

    /// Return the current hint unformatted to perform the completion of the full hint
    fn complete_hint(&self) -> String;

    /// Return the first semantic token of the hint
    /// for incremental completion
    fn next_hint_token(&self) -> String;
}

/// A default example hinter that use the completions or the history to show a hint to the user
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
    ) -> String {
        if line.chars().count() < self.min_chars {
            self.current_hint = String::new()
        } else {
            self.current_hint = history
                .iter_chronologic()
                .map(|x| x.entry.to_string())
                .rev()
                .find(|entry| entry.starts_with(line))
                .map_or_else(String::new, |entry| entry[line.len()..].to_string());
        }

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
        let mut reached_content = false;
        let result: String = self
            .current_hint
            .chars()
            .take_while(|c| match (c.is_whitespace(), reached_content) {
                (true, true) => false,
                (true, false) => true,
                (false, true) => true,
                (false, false) => {
                    reached_content = true;
                    true
                }
            })
            .collect();
        result
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
    pub fn with_style(mut self, style: Style) -> DefaultHinter {
        self.style = style;
        self
    }

    /// A builder that sets the number of characters that have to be present to enable history hints
    pub fn with_min_chars(mut self, min_chars: usize) -> DefaultHinter {
        self.min_chars = min_chars;
        self
    }
}
