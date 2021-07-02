use nu_ansi_term::Style;

pub struct StyledText {
    buffer: Vec<(Style, String)>,
}

impl Default for StyledText {
    fn default() -> Self {
        Self::new()
    }
}

impl StyledText {
    pub fn new() -> Self {
        Self { buffer: vec![] }
    }

    pub fn push(&mut self, styled_string: (Style, String)) {
        self.buffer.push(styled_string);
    }

    pub fn render_around_insertion_point(&self, insertion_point: usize) -> (String, String) {
        let mut current_idx = 0;
        let mut left_string = String::new();
        let mut right_string = String::new();

        for pair in &self.buffer {
            if current_idx >= insertion_point {
                right_string.push_str(&pair.0.paint(&pair.1).to_string());
            } else if pair.1.len() + current_idx <= insertion_point {
                left_string.push_str(&pair.0.paint(&pair.1).to_string());
            } else if pair.1.len() + current_idx > insertion_point {
                let offset = insertion_point - current_idx;

                let left_side = pair.1[..offset].to_string();
                let right_side = pair.1[offset..].to_string();

                left_string.push_str(&pair.0.paint(&left_side).to_string());
                right_string.push_str(&pair.0.paint(&right_side).to_string());
            }
            current_idx += pair.1.len();
        }

        (left_string, right_string)
    }
}
