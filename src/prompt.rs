use chrono::Local;
use crossterm::style::Color;
use std::env;

pub static DEFAULT_PROMPT_COLOR: Color = Color::Blue;
pub static DEFAULT_PROMPT_INDICATOR: &str = "〉";

/// API to provide a custom prompt.
///
/// Implementors have to provide [`String`]-based content which will be
/// displayed before the `LineBuffer` is drawn.
pub trait Prompt {
    /// Provide content off the full prompt. May use a line above the entry buffer that fits into `screen_width`.
    fn render_prompt(&self, screen_width: usize) -> String;
    /// Provide content of only the minimal prompt indicator in front of the buffer.
    fn render_prompt_indicator(&self) -> String;
    fn get_prompt_color(&self) -> Color;
}

impl Prompt for DefaultPrompt {
    fn render_prompt(&self, screen_width: usize) -> String {
        DefaultPrompt::render_prompt(self, screen_width)
    }

    fn render_prompt_indicator(&self) -> String {
        self.prompt_indicator.clone()
    }

    fn get_prompt_color(&self) -> Color {
        self.prompt_color
    }
}

impl Default for DefaultPrompt {
    fn default() -> Self {
        DefaultPrompt::new(DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR, 1)
    }
}

/// Simple two-line [`Prompt`] displaying the current working directory and the time above the entry line.
pub struct DefaultPrompt {
    prompt_color: Color,
    // The prompt symbol like >
    prompt_indicator: String,
    // The minimum number of line buffer character space between the
    // the left prompt and the right prompt. When this encroaches
    // into the right side prompt, we should not show the right
    // prompt.
    min_center_spacing: u16,
}

impl DefaultPrompt {
    pub fn new(
        prompt_color: Color,
        prompt_indicator: &str,
        min_center_spacing: u16,
    ) -> DefaultPrompt {
        DefaultPrompt {
            prompt_color,
            prompt_indicator: prompt_indicator.to_string(),
            min_center_spacing,
        }
    }

    // NOTE: This method currently assumes all characters are 1 column wide. This should be
    // ok for now since we're just displaying the current directory and date/time, which are
    // unlikely to contain characters that use 2 columns.
    pub fn render_prompt(&self, cols: usize) -> String {
        let mut prompt_str = String::new();

        let mut left_prompt = get_working_dir().unwrap_or_else(|_| String::from("no path"));
        left_prompt.truncate(cols);
        let left_prompt_width = left_prompt.chars().count();
        prompt_str.push_str(&left_prompt);

        let right_prompt = get_now();
        let right_prompt_width = right_prompt.chars().count();

        // Only print right prompt if there's enough room for it.
        if left_prompt_width + usize::from(self.min_center_spacing) + right_prompt_width <= cols {
            let right_prompt = format!("{:>width$}", get_now(), width = cols - left_prompt_width);
            prompt_str.push_str(&right_prompt);
        } else if left_prompt_width < cols {
            let right_padding = format!("{:>width$}", "", width = cols - left_prompt_width);
            prompt_str.push_str(&right_padding);
        }

        prompt_str.push_str(&self.prompt_indicator);

        prompt_str
    }
}

fn get_working_dir() -> Result<String, std::io::Error> {
    let path = env::current_dir()?;
    Ok(path.display().to_string())
}

fn get_now() -> String {
    let now = Local::now();
    format!("{}", now.format("%m/%d/%Y %I:%M:%S %p"))
}
