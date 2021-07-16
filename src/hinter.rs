use {
    crate::{Completer, DefaultCompleter, History},
    nu_ansi_term::{Color, Style},
};

pub trait Hinter {
    fn handle(&mut self, line: &str, pos: usize, history: &Box<dyn History>) -> String;
}

pub struct DefaultHinter {
    completer: Option<Box<dyn Completer>>,
    history: bool,
    style: Style,
    inside_line: bool,
}

impl Hinter for DefaultHinter {
    fn handle(&mut self, line: &str, pos: usize, history: &Box<dyn History>) -> String {
        let mut completions = vec![];
        let mut output = String::new();

        if (pos == line.len() && !self.inside_line) || self.inside_line {
            if let Some(c) = &self.completer {
                completions = c.complete(line, pos);
            } else if self.history {
                let history: Vec<String> = history.iter_chronologic().cloned().collect();
                completions = DefaultCompleter::new(history).complete(line, pos);
            }

            if !completions.is_empty() {
                let mut hint = completions[0].1.clone();
                let span = completions[0].0;
                hint.replace_range(0..(span.end - span.start), "");

                output = self.style.paint(hint).to_string();
            }
        }

        output
    }
}

impl Default for DefaultHinter {
    fn default() -> Self {
        DefaultHinter {
            completer: None,
            history: false,
            style: Style::new().fg(Color::LightGray),
            inside_line: false,
        }
    }
}

impl DefaultHinter {
    pub fn with_inside_line(mut self) -> DefaultHinter {
        self.inside_line = true;
        self
    }

    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> DefaultHinter {
        self.completer = Some(completer);
        self
    }
    pub fn with_history(mut self) -> DefaultHinter {
        self.history = true;
        self
    }
    pub fn with_style(mut self, style: Style) -> DefaultHinter {
        self.style = style;
        self
    }
}
