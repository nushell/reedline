use nu_ansi_term::Style;

use crate::Prompt;

use super::utils::strip_ansi;

/// A representation of a buffer with styling, used for doing syntax highlighting
#[derive(Clone)]
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
    pub const fn new() -> Self {
        Self { buffer: vec![] }
    }

    /// Add a new styled string to the buffer
    pub fn push(&mut self, styled_string: (Style, String)) {
        self.buffer.push(styled_string);
    }

    /// Style range with the provided style
    pub fn style_range(&mut self, from: usize, to: usize, new_style: Style) {
        let (from, to) = if from > to { (to, from) } else { (from, to) };
        let mut current_idx = 0;
        let mut pair_idx = 0;
        while pair_idx < self.buffer.len() {
            let pair = &mut self.buffer[pair_idx];
            let end_idx = current_idx + pair.1.len();
            enum Position {
                Before,
                In,
                After,
            }
            let start_position = if current_idx < from {
                Position::Before
            } else if current_idx >= to {
                Position::After
            } else {
                Position::In
            };
            let end_position = if end_idx < from {
                Position::Before
            } else if end_idx > to {
                Position::After
            } else {
                Position::In
            };
            match (start_position, end_position) {
                (Position::Before, Position::After) => {
                    let mut in_range = pair.1.split_off(from - current_idx);
                    let after_range = in_range.split_off(to - from);
                    let in_range = (new_style, in_range);
                    let after_range = (pair.0, after_range);
                    self.buffer.insert(pair_idx + 1, in_range);
                    self.buffer.insert(pair_idx + 2, after_range);
                    break;
                }
                (Position::Before, Position::In) => {
                    let in_range = pair.1.split_off(from - current_idx);
                    pair_idx += 1; // Additional increment for the split pair, since the new insertion is already correctly styled and can be skipped next iteration
                    self.buffer.insert(pair_idx, (new_style, in_range));
                }
                (Position::In, Position::After) => {
                    let after_range = pair.1.split_off(to - current_idx);
                    let old_style = pair.0;
                    pair.0 = new_style;
                    if !after_range.is_empty() {
                        self.buffer.insert(pair_idx + 1, (old_style, after_range));
                    }
                    break;
                }
                (Position::In, Position::In) => pair.0 = new_style,

                (Position::After, _) => break,
                _ => (),
            }
            current_idx = end_idx;
            pair_idx += 1;
        }
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
        prompt: &dyn Prompt,
        // multiline_prompt: &str,
        use_ansi_coloring: bool,
    ) -> (String, String) {
        let mut current_idx = 0;
        let mut left_string = String::new();
        let mut right_string = String::new();

        let multiline_prompt = prompt.render_prompt_multiline_indicator();
        let prompt_style = Style::new().fg(prompt.get_prompt_multiline_color());

        for pair in &self.buffer {
            if current_idx >= insertion_point {
                right_string.push_str(&render_as_string(pair, &prompt_style, &multiline_prompt));
            } else if pair.1.len() + current_idx <= insertion_point {
                left_string.push_str(&render_as_string(pair, &prompt_style, &multiline_prompt));
            } else if pair.1.len() + current_idx > insertion_point {
                let offset = insertion_point - current_idx;

                let left_side = pair.1[..offset].to_string();
                let right_side = pair.1[offset..].to_string();

                left_string.push_str(&render_as_string(
                    &(pair.0, left_side),
                    &prompt_style,
                    &multiline_prompt,
                ));
                right_string.push_str(&render_as_string(
                    &(pair.0, right_side),
                    &prompt_style,
                    &multiline_prompt,
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
    let formatted_multiline_prompt = format!("\n{multiline_prompt}");
    for (line_number, line) in renderable.1.split('\n').enumerate() {
        if line_number != 0 {
            rendered.push_str(&prompt_style.paint(&formatted_multiline_prompt).to_string());
        }
        rendered.push_str(&renderable.0.paint(line).to_string());
    }
    rendered
}

#[cfg(test)]
mod test {
    use nu_ansi_term::{Color, Style};

    use crate::StyledText;

    fn get_styled_text_template() -> (super::StyledText, Style, Style) {
        let before_style = Style::new().on(Color::Black);
        let after_style = Style::new().on(Color::Red);
        (
            super::StyledText {
                buffer: vec![
                    (before_style, "aaa".into()),
                    (before_style, "bbb".into()),
                    (before_style, "ccc".into()),
                ],
            },
            before_style,
            after_style,
        )
    }
    #[test]
    fn style_range_partial_update_one_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template.clone();
        styled_text.style_range(0, 1, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "a".into()));
        assert_eq!(styled_text.buffer[1], (before_style, "aa".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "bbb".into()));
        assert_eq!(styled_text.buffer[3], (before_style, "ccc".into()));
    }
    #[test]
    fn style_range_complete_update_one_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template.clone();
        styled_text.style_range(0, 3, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "aaa".into()));
        assert_eq!(styled_text.buffer[1], (before_style, "bbb".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "ccc".into()));
        assert_eq!(styled_text.buffer.len(), 3);
    }
    #[test]
    fn style_range_update_over_boundary() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template;
        styled_text.style_range(0, 5, after_style);
        assert_eq!(styled_text.buffer[0], (after_style, "aaa".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "bb".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "b".into()));
        assert_eq!(styled_text.buffer[3], (before_style, "ccc".into()));
    }
    #[test]
    fn style_range_update_over_part() {
        let (styled_text_template, before_style, after_style) = get_styled_text_template();
        let mut styled_text = styled_text_template;
        styled_text.style_range(1, 7, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "a".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "aa".into()));
        assert_eq!(styled_text.buffer[2], (after_style, "bbb".into()));
        assert_eq!(styled_text.buffer[3], (after_style, "c".into()));
        assert_eq!(styled_text.buffer[4], (before_style, "cc".into()));
    }
    #[test]
    fn style_range_last_letter() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![(before_style, "asdf".into())],
        };
        styled_text.style_range(3, 4, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "asd".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "f".into()));
    }
    #[test]
    fn style_range_from_second_to_last() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![(before_style, "asdf".into())],
        };
        styled_text.style_range(2, 3, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "as".into()));
        assert_eq!(styled_text.buffer[1], (after_style, "d".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "f".into()));
    }
    #[test]
    fn regression_style_range_cargo_run() {
        let (_, before_style, after_style) = get_styled_text_template();
        let mut styled_text = StyledText {
            buffer: vec![
                (before_style, "cargo".into()),
                (before_style, " ".into()),
                (before_style, "run".into()),
            ],
        };
        styled_text.style_range(8, 7, after_style);
        assert_eq!(styled_text.buffer[0], (before_style, "cargo".into()));
        assert_eq!(styled_text.buffer[1], (before_style, " ".into()));
        assert_eq!(styled_text.buffer[2], (before_style, "r".into()));
        assert_eq!(styled_text.buffer[3], (after_style, "u".into()));
        assert_eq!(styled_text.buffer[4], (before_style, "n".into()));
    }
}
