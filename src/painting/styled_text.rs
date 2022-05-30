use super::utils::strip_ansi;
use nu_ansi_term::{Color, Style};

/// A representation of a buffer with styling, used for doing syntax highlighting
pub struct StyledText {
    /// The component, styled parts of the text
    pub buffer: Vec<(Style, String)>,
}

impl Default for StyledText {
    fn default() -> Self {
        Self::new()
    }
}

impl StyledText {
    /// Construct a new `StyledText`
    pub fn new() -> Self {
        Self { buffer: vec![] }
    }

    /// Add a new styled string to the buffer
    pub fn push(&mut self, styled_string: (Style, String)) {
        self.buffer.push(styled_string);
    }

    /// Render the styled string. We use the insertion point to render around so that
    /// we can properly write out the styled string to the screen and find the correct
    /// place to put the cursor. This assumes a logic that prints the first part of the
    /// string, saves the cursor position, prints the second half, and then restores
    /// the cursor position
    ///
    /// Also inserts the multiline continuation prompt
    pub fn render_around_insertion_point(
        &self,
        insertion_point: usize,
        multiline_prompt: &str,
        use_ansi_coloring: bool,
    ) -> (String, String) {
        let mut current_idx = 0;
        let mut left_string = String::new();
        let mut right_string = String::new();
        let prompt_style = Style::new().fg(Color::LightBlue);
        for pair in &self.buffer {
            if current_idx >= insertion_point {
                right_string.push_str(&render_as_string(pair, &prompt_style, multiline_prompt));
            } else if pair.1.len() + current_idx <= insertion_point {
                left_string.push_str(&render_as_string(pair, &prompt_style, multiline_prompt));
            } else if pair.1.len() + current_idx > insertion_point {
                let offset = insertion_point - current_idx;

                let left_side = pair.1[..offset].to_string();
                let right_side = pair.1[offset..].to_string();

                left_string.push_str(&render_as_string(
                    &(pair.0, left_side),
                    &prompt_style,
                    multiline_prompt,
                ));
                right_string.push_str(&render_as_string(
                    &(pair.0, right_side),
                    &prompt_style,
                    multiline_prompt,
                ));
            }
            current_idx += pair.1.len();
        }

        if use_ansi_coloring {
            (left_string, right_string)
        } else {
            (strip_ansi(&left_string), strip_ansi(&right_string))
        }
    }

    /// Apply the ANSI style formatting to the full string.
    pub fn render_simple(&self) -> String {
        self.buffer
            .iter()
            .map(|(style, text)| style.paint(text).to_string())
            .collect()
    }

    /// Get the unformatted text as a single continuous string.
    pub fn raw_string(&self) -> String {
        self.buffer.iter().map(|(_, str)| str.as_str()).collect()
    }
}

fn render_as_string(
    renderable: &(Style, String),
    prompt_style: &Style,
    multiline_prompt: &str,
) -> String {
    let mut rendered = String::new();
    let formatted_multiline_prompt = format!("\n{}", multiline_prompt);
    for (line_number, line) in renderable.1.split('\n').enumerate() {
        if line_number != 0 {
            rendered.push_str(&prompt_style.paint(&formatted_multiline_prompt).to_string());
        }
        rendered.push_str(&renderable.0.paint(line).to_string());
    }
    rendered
}
