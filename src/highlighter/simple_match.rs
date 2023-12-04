use crate::highlighter::Highlighter;
use crate::StyledText;
use nu_ansi_term::{Color, Style};

/// Highlight all matches for a given search string in a line
///
/// Default style:
///
/// - non-matching text: Default style
/// - matching text: Green foreground color
pub struct SimpleMatchHighlighter {
    neutral_style: Style,
    match_style: Style,
    query: String,
}

impl Default for SimpleMatchHighlighter {
    fn default() -> Self {
        Self {
            neutral_style: Style::default(),
            match_style: Style::new().fg(Color::Green),
            query: String::default(),
        }
    }
}

impl Highlighter for SimpleMatchHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled_text = StyledText::new();
        if self.query.is_empty() {
            styled_text.push((self.neutral_style, line.to_owned()));
        } else {
            let mut next_idx: usize = 0;

            for (idx, mat) in line.match_indices(&self.query) {
                if idx != next_idx {
                    styled_text.push((self.neutral_style, line[next_idx..idx].to_owned()));
                }
                styled_text.push((self.match_style, mat.to_owned()));
                next_idx = idx + mat.len();
            }
            if next_idx != line.len() {
                styled_text.push((self.neutral_style, line[next_idx..].to_owned()));
            }
        }
        styled_text
    }
}

impl SimpleMatchHighlighter {
    /// Create a simple highlighter that styles every exact match of `query`.
    pub fn new(query: String) -> Self {
        Self {
            query,
            ..Self::default()
        }
    }

    /// Update query string to match
    #[must_use]
    pub fn with_query(mut self, query: String) -> Self {
        self.query = query;
        self
    }

    /// Set style for the matches found
    #[must_use]
    pub fn with_match_style(mut self, match_style: Style) -> Self {
        self.match_style = match_style;
        self
    }

    /// Set style for the text that does not match the query
    #[must_use]
    pub fn with_neutral_style(mut self, neutral_style: Style) -> Self {
        self.neutral_style = neutral_style;
        self
    }
}
