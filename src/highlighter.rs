use {
    nu_ansi_term::{Color, Style},
    std::borrow::Cow,
};

pub static DEFAULT_BUFFER_MATCH_COLOR: Color = Color::Green;
pub static DEFAULT_BUFFER_NEUTRAL_COLOR: Color = Color::White;
pub static DEFAULT_BUFFER_NOTMATCH_COLOR: Color = Color::Red;

pub trait Highlighter {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str>;
}

pub struct DefaultHighlighter {
    external_commands: Vec<String>,
    match_color: Color,
    notmatch_color: Color,
    neutral_color: Color,
}

impl Highlighter for DefaultHighlighter {
    fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        if self
            .external_commands
            .clone()
            .iter()
            .any(|x| line.contains(x))
        {
            let matches: Vec<String> = self
                .external_commands
                .iter()
                .filter(|c| line.contains(*c))
                .map(|c| c.to_string())
                .collect();
            let longest_match = matches.iter().fold("".to_string(), |acc, item| {
                if item.len() > acc.len() {
                    item.clone()
                } else {
                    acc
                }
            });
            let buffer_split: Vec<&str> = line.splitn(2, &longest_match).collect();
            Cow::Owned(format!(
                "{}{}{}",
                Style::from(self.neutral_color).paint(buffer_split[0]),
                Style::from(self.match_color).paint(&longest_match),
                Style::from(self.neutral_color).paint(buffer_split[1])
            ))
        } else if !self.external_commands.is_empty() {
            Cow::Owned(Style::from(self.notmatch_color).paint(line).to_string())
        } else {
            Cow::Owned(Style::from(self.neutral_color).paint(line).to_string())
        }
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
