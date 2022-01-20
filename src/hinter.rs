use {
    crate::History,
    nu_ansi_term::{Color, Style},
};

/// A trait that's responsible for returning the hint for the current line and position
/// Hints are often shown in-line as part of the buffer, showing the user text they can accept or ignore
pub trait Hinter: Send {
    /// Handle the hinting duty by using the line, position, and current history
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
    ) -> String;

    /// Return the current hint being shown to the user
    fn current_hint(&self) -> String;
}

/// A default example hinter that use the completions or the history to show a hint to the user
pub struct DefaultHinter {
    style: Style,
    current_hint: String,
}

impl Hinter for DefaultHinter {
    fn handle(
        &mut self,
        line: &str,
        #[allow(unused_variables)] pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
    ) -> String {
        self.current_hint = history
            .iter_chronologic()
            .rev()
            .find(|entry| entry.starts_with(line))
            .map_or_else(String::new, |entry| entry[line.len()..].to_string());

        if use_ansi_coloring {
            self.style.paint(&self.current_hint).to_string()
        } else {
            self.current_hint.clone()
        }
    }

    fn current_hint(&self) -> String {
        self.current_hint.clone()
    }
}

impl Default for DefaultHinter {
    fn default() -> Self {
        DefaultHinter {
            style: Style::new().fg(Color::LightGray),
            current_hint: String::new(),
        }
    }
}

impl DefaultHinter {
    /// A builder that sets the style applied to the hint as part of the buffer
    pub fn with_style(mut self, style: Style) -> DefaultHinter {
        self.style = style;
        self
    }
}
