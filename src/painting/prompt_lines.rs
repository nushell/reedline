use super::utils::{coerce_crlf, estimate_required_lines, line_width};
use crate::{
    menu::{Menu, ReedlineMenu},
    prompt::PromptEditMode,
    Prompt, PromptHistorySearch,
};
use std::borrow::Cow;

/// Aggregate of prompt and input string used by `Painter`
pub(crate) struct PromptLines<'prompt> {
    pub(crate) prompt_str_left: Cow<'prompt, str>,
    pub(crate) prompt_str_right: Cow<'prompt, str>,
    pub(crate) prompt_indicator: Cow<'prompt, str>,
    pub(crate) before_cursor: Cow<'prompt, str>,
    pub(crate) after_cursor: Cow<'prompt, str>,
    pub(crate) hint: Cow<'prompt, str>,
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

        Self {
            prompt_str_left,
            prompt_str_right,
            prompt_indicator,
            before_cursor,
            after_cursor,
            hint,
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

    /// Total lines that the prompt uses considering that it may wrap the screen
    pub(crate) fn prompt_lines_with_wrap(&self, screen_width: u16) -> u16 {
        let complete_prompt = self.prompt_str_left.to_string() + &self.prompt_indicator;
        let lines = estimate_required_lines(&complete_prompt, screen_width);
        lines.saturating_sub(1) as u16
    }

    /// Estimated width of the actual input
    pub(crate) fn estimate_first_input_line_width(&self) -> u16 {
        let last_line_left_prompt = self.prompt_str_left.lines().last();

        let prompt_lines_total = self.before_cursor.to_string() + &self.after_cursor + &self.hint;
        let prompt_lines_first = prompt_lines_total.lines().next();

        let mut estimate = 0; // space in front of the input

        if let Some(last_line_left_prompt) = last_line_left_prompt {
            estimate += line_width(last_line_left_prompt);
        }

        estimate += line_width(&self.prompt_indicator);

        if let Some(prompt_lines_first) = prompt_lines_first {
            estimate += line_width(prompt_lines_first);
        }

        if estimate > u16::MAX as usize {
            u16::MAX
        } else {
            estimate as u16
        }
    }
}
