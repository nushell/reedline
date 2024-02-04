use crate::highlighter::Highlighter;
use crate::StyledText;
use nu_ansi_term::{Color, Style};

pub static DEFAULT_BUFFER_MATCH_COLOR: Color = Color::Green;
pub static DEFAULT_BUFFER_NEUTRAL_COLOR: Color = Color::White;
pub static DEFAULT_BUFFER_NOT_MATCH_COLOR: Color = Color::Red;

/// A simple, example highlighter that shows how to highlight keywords
pub struct ExampleHighlighter {
    external_commands: Vec<String>,
    match_color: Color,
    not_match_color: Color,
    neutral_color: Color,
}

impl Highlighter for ExampleHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled_text = StyledText::new();

        if self
            .external_commands
            .clone()
            .iter()
            .any(|x| line.contains(x))
        {
            let matches: Vec<&str> = self
                .external_commands
                .iter()
                .filter(|c| line.contains(*c))
                .map(std::ops::Deref::deref)
                .collect();
            let longest_match = matches.iter().fold("".to_string(), |acc, &item| {
                if item.len() > acc.len() {
                    item.to_string()
                } else {
                    acc
                }
            });
            let buffer_split: Vec<&str> = line.splitn(2, &longest_match).collect();

            styled_text.push((
                Style::new().fg(self.neutral_color),
                buffer_split[0].to_string(),
            ));
            styled_text.push((Style::new().fg(self.match_color), longest_match));
            styled_text.push((
                Style::new().bold().fg(self.neutral_color),
                buffer_split[1].to_string(),
            ));
        } else if self.external_commands.is_empty() {
            styled_text.push((Style::new().fg(self.neutral_color), line.to_string()));
        } else {
            styled_text.push((Style::new().fg(self.not_match_color), line.to_string()));
        }

        styled_text
    }
}
impl ExampleHighlighter {
    /// Construct the default highlighter with a given set of extern commands/keywords to detect and highlight
    pub fn new(external_commands: Vec<String>) -> ExampleHighlighter {
        ExampleHighlighter {
            external_commands,
            match_color: DEFAULT_BUFFER_MATCH_COLOR,
            not_match_color: DEFAULT_BUFFER_NOT_MATCH_COLOR,
            neutral_color: DEFAULT_BUFFER_NEUTRAL_COLOR,
        }
    }

    /// Configure the highlighter to use different colors
    pub fn change_colors(
        &mut self,
        match_color: Color,
        notmatch_color: Color,
        neutral_color: Color,
    ) {
        self.match_color = match_color;
        self.not_match_color = notmatch_color;
        self.neutral_color = neutral_color;
    }
}
impl Default for ExampleHighlighter {
    fn default() -> Self {
        ExampleHighlighter::new(vec![])
    }
}
