use super::utils::{coerce_crlf, estimate_required_lines, line_width};
use crate::{
    menu::{Menu, ReedlineMenu},
    prompt::PromptEditMode,
    Prompt, PromptHistorySearch,
};
use std::borrow::Cow;

/// Aggregate of prompt and input string used by `Painter`
#[derive(Debug)]
pub(crate) struct PromptLines<'prompt> {
    pub(crate) prompt_str_left: Cow<'prompt, str>,
    pub(crate) prompt_str_right: Cow<'prompt, str>,
    pub(crate) prompt_indicator: Cow<'prompt, str>,
    pub(crate) before_cursor: Cow<'prompt, str>,
    pub(crate) after_cursor: Cow<'prompt, str>,
    pub(crate) hint: Cow<'prompt, str>,
    pub(crate) right_prompt_on_last_line: bool,
}

impl<'prompt> PromptLines<'prompt> {
    /// Splits the strings before and after the cursor as well as the hint
    /// This vector with the str are used to calculate how many lines are
    /// required to print after the prompt
    pub fn new(
        prompt: &'prompt dyn Prompt,
        prompt_mode: PromptEditMode,
        history_indicator: Option<PromptHistorySearch>,
        before_cursor: &'prompt str,
        after_cursor: &'prompt str,
        hint: &'prompt str,
    ) -> Self {
        let prompt_str_left = prompt.render_prompt_left();
        let prompt_str_right = prompt.render_prompt_right();

        let prompt_indicator = match history_indicator {
            Some(prompt_search) => prompt.render_prompt_history_search_indicator(prompt_search),
            None => prompt.render_prompt_indicator(prompt_mode),
        };

        let before_cursor = coerce_crlf(before_cursor);
        let after_cursor = coerce_crlf(after_cursor);
        let hint = coerce_crlf(hint);
        let right_prompt_on_last_line = prompt.right_prompt_on_last_line();

        Self {
            prompt_str_left,
            prompt_str_right,
            prompt_indicator,
            before_cursor,
            after_cursor,
            hint,
            right_prompt_on_last_line,
        }
    }

    /// The required lines to paint the buffer are calculated by counting the
    /// number of newlines in all the strings that form the prompt and buffer.
    /// The plus 1 is to indicate that there should be at least one line.
    pub(crate) fn required_lines(&self, terminal_columns: u16, menu: Option<&ReedlineMenu>) -> u16 {
        let input = if menu.is_none() {
            self.prompt_str_left.to_string()
                + &self.prompt_indicator
                + &self.before_cursor
                + &self.after_cursor
                + &self.hint
        } else {
            self.prompt_str_left.to_string()
                + &self.prompt_indicator
                + &self.before_cursor
                + &self.after_cursor
        };

        let lines = estimate_required_lines(&input, terminal_columns);

        if let Some(menu) = menu {
            lines as u16 + menu.menu_required_lines(terminal_columns)
        } else {
            lines as u16
        }
    }

    /// Estimated distance of the cursor to the prompt.
    /// This considers line wrapping
    pub(crate) fn distance_from_prompt(&self, terminal_columns: u16) -> u16 {
        let input = self.prompt_str_left.to_string() + &self.prompt_indicator + &self.before_cursor;
        let lines = estimate_required_lines(&input, terminal_columns);
        lines.saturating_sub(1) as u16
    }

    /// Calculate the cursor pos, based on the buffer and prompt.
    /// The height is relative to the prompt
    pub(crate) fn cursor_pos(&self, terminal_columns: u16) -> (u16, u16) {
        // If we have a multiline prompt (e.g starship), we expect the cursor to be on the last line
        let prompt_str = format!("{}{}", self.prompt_str_left, self.prompt_indicator);
        // The Cursor position will be relative to this
        let last_prompt_str = prompt_str.lines().last().unwrap_or_default();

        let is_multiline = self.before_cursor.contains('\n');
        let buffer_width = line_width(self.before_cursor.lines().last().unwrap_or_default());

        let total_width = if is_multiline {
            // The buffer already contains the multiline prompt
            buffer_width
        } else {
            buffer_width + line_width(last_prompt_str)
        };

        let buffer_width_prompt = format!("{}{}", last_prompt_str, self.before_cursor);

        let cursor_y = (estimate_required_lines(&buffer_width_prompt, terminal_columns) as u16)
            .saturating_sub(1); // 0 based

        let cursor_x = (total_width % terminal_columns as usize) as u16;

        (cursor_x, cursor_y as u16)
    }

    /// Total lines that the prompt uses considering that it may wrap the screen
    pub(crate) fn prompt_lines_with_wrap(&self, screen_width: u16) -> u16 {
        let complete_prompt = self.prompt_str_left.to_string() + &self.prompt_indicator;
        let lines = estimate_required_lines(&complete_prompt, screen_width);
        lines.saturating_sub(1) as u16
    }

    /// Estimated width of the line where right prompt will be rendered
    pub(crate) fn estimate_right_prompt_line_width(&self, terminal_columns: u16) -> u16 {
        let first_line_left_prompt = self.prompt_str_left.lines().next();
        let last_line_left_prompt = self.prompt_str_left.lines().last();

        let prompt_lines_total = self.before_cursor.to_string() + &self.after_cursor + &self.hint;
        let prompt_lines_first = prompt_lines_total.lines().next();

        let mut estimate = 0; // space in front of the input

        if self.right_prompt_on_last_line {
            if let Some(last_line_left_prompt) = last_line_left_prompt {
                estimate += line_width(last_line_left_prompt);
                estimate += line_width(&self.prompt_indicator);

                if let Some(prompt_lines_first) = prompt_lines_first {
                    estimate += line_width(prompt_lines_first);
                }
            }
        } else {
            // Render right prompt on the first line
            let required_lines = estimate_required_lines(&self.prompt_str_left, terminal_columns);
            if let Some(first_line_left_prompt) = first_line_left_prompt {
                estimate += line_width(first_line_left_prompt);
            }

            // A single line
            if required_lines == 1 {
                estimate += line_width(&self.prompt_indicator);

                if let Some(prompt_lines_first) = prompt_lines_first {
                    estimate += line_width(prompt_lines_first);
                }
            }
        }

        if estimate > u16::MAX as usize {
            u16::MAX
        } else {
            estimate as u16
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case(
        "~/path/",
        "❯ ",
        "",
        100,
        (9, 0)
    )]
    #[case(
        "~/longer/path/\n",
        "❯ ",
        "test",
        100,
        (6, 0)
    )]
    #[case(
        "~/longer/path/",
        "\n❯ ",
        "test",
        100,
        (6, 0)
    )]
    #[case(
        "~/longer/path/\n",
        "\n❯ ",
        "test",
        100,
        (6, 0)
    )]
    #[case(
        "~/path/",
        "❯ ",
        "very long input that does not fit in a single line",
        40,
        (19, 1)
    )]
    #[case(
        "~/path/\n",
        "\n❯\n ",
        "very long input that does not fit in a single line",
        10,
        (1, 5)
    )]
    #[case(
        "~/path/",
        "❯ ",
        "this is a text that contains newlines\n::: and a multiline prompt",
        40,
        (26, 2)
    )]
    #[case(
        "~/path/",
        "❯ ",
        "this is a text that contains newlines\n::: and very loooooooooooooooong text that wraps",
        40,
        (8, 3)
    )]

    fn test_cursor_pos(
        #[case] prompt_str_left: &str,
        #[case] prompt_indicator: &str,
        #[case] before_cursor: &str,
        #[case] terminal_columns: u16,
        #[case] expected: (u16, u16),
    ) {
        let prompt_lines = PromptLines {
            prompt_str_left: Cow::Borrowed(prompt_str_left),
            prompt_str_right: Cow::Borrowed(""),
            prompt_indicator: Cow::Borrowed(prompt_indicator),
            before_cursor: Cow::Borrowed(before_cursor),
            after_cursor: Cow::Borrowed(""),
            hint: Cow::Borrowed(""),
            right_prompt_on_last_line: false,
        };

        let pos = prompt_lines.cursor_pos(terminal_columns);

        assert_eq!(pos, expected);
    }
}
