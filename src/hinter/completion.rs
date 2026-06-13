use crate::{hinter::get_first_token, Completer, Hinter, History};
use nu_ansi_term::{Color, Style};
use std::sync::{Arc, Mutex};

/// A hinter that uses completions (not history) to show inline suggestions
///
/// This provides fish-style autosuggestions based on what the completer returns,
/// showing the first completion result as gray text that can be accepted with →
pub struct CompletionHinter {
    completer: Arc<Mutex<dyn Completer + Send>>,
    style: Style,
    current_hint: String,
    min_chars: usize,
}

impl Hinter for CompletionHinter {
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        _history: &dyn History,
        use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        self.current_hint = if line.chars().count() >= self.min_chars && pos == line.len() {
            // Only show hints when cursor is at end of line
            if let Ok(mut completer) = self.completer.lock() {
                let suggestions = completer.complete(line, pos);
                if let Some(first) = suggestions.first() {
                    // The suggestion replaces line[span.start..span.end] with `value`
                    // We want to show what extends beyond what the user typed
                    let span_end = first.span.end.min(line.len());
                    let span_start = first.span.start.min(span_end);
                    let typed_portion = &line[span_start..span_end];

                    // If the completion value starts with what's being replaced,
                    // show the suffix (the new part)
                    if let Some(suffix) = first.value.strip_prefix(typed_portion) {
                        suffix.to_string()
                    } else {
                        // Fuzzy match - just show if value is longer than typed
                        if first.value.len() > typed_portion.len() {
                            first.value.clone()
                        } else {
                            String::new()
                        }
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
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
}

impl CompletionHinter {
    /// Create a new CompletionHinter with the given completer
    pub fn new(completer: Arc<Mutex<dyn Completer + Send>>) -> Self {
        CompletionHinter {
            completer,
            style: Style::new().fg(Color::DarkGray),
            current_hint: String::new(),
            min_chars: 1,
        }
    }

    /// A builder that sets the style applied to the hint
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// A builder that sets minimum characters before showing hints
    #[must_use]
    pub fn with_min_chars(mut self, min_chars: usize) -> Self {
        self.min_chars = min_chars;
        self
    }
}
