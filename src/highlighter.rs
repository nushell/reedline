use nu_ansi_term::Style;

use {crate::styled_text::StyledText, nu_ansi_term::Color};

pub static DEFAULT_BUFFER_MATCH_COLOR: Color = Color::Green;
pub static DEFAULT_BUFFER_NEUTRAL_COLOR: Color = Color::White;
pub static DEFAULT_BUFFER_NOTMATCH_COLOR: Color = Color::Red;

pub trait Highlighter {
    fn highlight(&self, line: &str) -> StyledText;
}

pub struct DefaultHighlighter {
    external_commands: Vec<String>,
    match_color: Color,
    notmatch_color: Color,
    neutral_color: Color,
}

impl Highlighter for DefaultHighlighter {
    fn highlight(&self, line: &str) -> StyledText {
        let mut styled_text = StyledText::new();
        let lowercase_line = line.to_lowercase();

        if self
            .external_commands
            .clone()
            .iter()
            .any(|x| lowercase_line.contains(x))
        {
            let matches: Vec<String> = self
                .external_commands
                .iter()
                .filter(|c| lowercase_line.contains(*c))
                .map(|c| c.to_string())
                .collect();
            let longest_match = matches.iter().fold("".to_string(), |acc, item| {
                if item.len() > acc.len() {
                    item.clone()
                } else {
                    acc
                }
            });
            let lowercase_buffer_split: Vec<&str> =
                lowercase_line.splitn(2, &longest_match).collect();
            let before_longest_match = line[..lowercase_buffer_split[0].len()].to_string();
            let after_longest_match =
                line[(lowercase_buffer_split[0].len() + longest_match.len())..].to_string();

            styled_text.push((Style::new().fg(self.neutral_color), before_longest_match));
            styled_text.push((
                Style::new().fg(self.match_color),
                line[lowercase_buffer_split[0].len()
                    ..(lowercase_buffer_split[0].len() + longest_match.len())]
                    .to_string(),
            ));
            styled_text.push((
                Style::new().bold().fg(self.neutral_color),
                after_longest_match,
            ));
        } else if !self.external_commands.is_empty() {
            styled_text.push((Style::new().fg(self.notmatch_color), line.to_string()));
        } else {
            styled_text.push((Style::new().fg(self.neutral_color), line.to_string()));
        }

        styled_text
    }
}
impl DefaultHighlighter {
    pub fn new(external_commands: Vec<String>) -> DefaultHighlighter {
        DefaultHighlighter {
            external_commands,
            match_color: DEFAULT_BUFFER_MATCH_COLOR,
            notmatch_color: DEFAULT_BUFFER_NOTMATCH_COLOR,
            neutral_color: DEFAULT_BUFFER_NEUTRAL_COLOR,
        }
    }
    pub fn change_colors(
        &mut self,
        match_color: Color,
        notmatch_color: Color,
        neutral_color: Color,
    ) {
        self.match_color = match_color;
        self.notmatch_color = notmatch_color;
        self.neutral_color = neutral_color;
    }
}
impl Default for DefaultHighlighter {
    fn default() -> Self {
        DefaultHighlighter::new(vec![])
    }
}
