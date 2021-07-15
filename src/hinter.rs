use {
    crate::{Completer, DefaultCompleter, History},
    nu_ansi_term::{Color, Style},
};

pub trait Hinter {
    fn handle(&mut self, line: &str, pos: usize) -> String;
}

pub struct DefaultHinter {
    completer: Option<Box<dyn Completer>>,
    history: Option<Box<dyn History>>,
    style: Style,
}

impl Hinter for DefaultHinter {
    fn handle(&mut self, line: &str, pos: usize) -> String {
        let mut completions = vec![];
        let mut output = String::new();

        if let Some(c) = &self.completer {
            completions = c.complete(line, pos);
        } else if let Some(h) = &self.history {
            let history: Vec<String> = h.iter_chronologic().cloned().collect();
            completions = DefaultCompleter::new(history).complete(line, pos);
        }

        if !completions.is_empty() {
            let mut hint = completions[0].1.clone();
            let span = completions[0].0;
            hint.replace_range(0..(span.end - span.start), "");

            output = self.style.paint(hint).to_string();
        }
        output
    }
}

impl Default for DefaultHinter {
    fn default() -> Self {
        DefaultHinter {
            completer: None,
            history: None,
            style: Style::new().fg(Color::LightGray),
        }
    }
}

impl DefaultHinter {
    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> DefaultHinter {
        self.completer = Some(completer);
        self
    }
    pub fn with_history(mut self, history: Box<dyn History>) -> DefaultHinter {
        self.history = Some(history);
        self
    }
    pub fn with_style(mut self, style: Style) -> DefaultHinter {
        self.style = style;
        self
    }
}
