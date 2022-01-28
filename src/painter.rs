use {
    crate::{
        menu::Menu, prompt::PromptEditMode, styled_text::strip_ansi, Prompt, PromptHistorySearch,
    },
    crossterm::{
        cursor::{self, MoveTo, RestorePosition, SavePosition},
        style::{Print, ResetColor, SetForegroundColor},
        terminal::{self, Clear, ClearType, ScrollUp},
        QueueableCommand, Result,
    },
    std::borrow::Cow,
    std::io::Write,
    unicode_width::UnicodeWidthStr,
};

#[derive(Default)]
struct PromptCoordinates {
    prompt_start: (u16, u16),
}

impl PromptCoordinates {
    fn set_prompt_start(&mut self, col: u16, row: u16) {
        self.prompt_start = (col, row);
    }
}

pub struct PromptLines<'prompt> {
    prompt_str_left: Cow<'prompt, str>,
    prompt_str_right: Cow<'prompt, str>,
    prompt_indicator: Cow<'prompt, str>,
    before_cursor: Cow<'prompt, str>,
    after_cursor: Cow<'prompt, str>,
    hint: Cow<'prompt, str>,
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
    fn required_lines(&self, terminal_columns: u16, menu: Option<&dyn Menu>) -> u16 {
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

        let lines = input.lines().fold(0, |acc, line| {
            let wrap = estimated_wrapped_line_count(line, terminal_columns);

            acc + 1 + wrap
        });

        if let Some(menu) = menu {
            let wrap_lines = menu.get_values().iter().fold(0, |acc, (_, line)| {
                let wrap = estimated_wrapped_line_count(line, terminal_columns);

                acc + wrap
            });

            lines as u16 + menu.get_rows() + wrap_lines as u16
        } else {
            lines as u16
        }
    }

    /// Estimated distance of the cursor to the prompt.
    /// This considers line wrapping
    fn distance_from_prompt(&self, terminal_columns: u16) -> u16 {
        let input = self.prompt_str_left.to_string() + &self.prompt_indicator + &self.before_cursor;

        let lines = input.lines().fold(0, |acc, line| {
            let wrap = estimated_wrapped_line_count(line, terminal_columns);

            acc + 1 + wrap
        });

        lines.saturating_sub(1) as u16
    }

    fn concatenate_lines(&self) -> String {
        self.before_cursor.to_string() + &self.after_cursor + &self.hint
    }

    /// Total lines that the prompt uses considering that it may wrap the screen
    fn prompt_lines_with_wrap(&self, screen_width: u16) -> u16 {
        let complete_prompt = self.prompt_str_left.to_string() + &self.prompt_indicator;
        let prompt_wrap = estimated_wrapped_line_count(&complete_prompt, screen_width);

        (self.prompt_str_left.matches('\n').count() + prompt_wrap) as u16
    }

    /// Estimated width of the actual input
    fn estimate_first_input_line_width(&self) -> u16 {
        let last_line_left_prompt = self.prompt_str_left.lines().last();

        let prompt_lines_total = self.concatenate_lines();
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

/// Reports the additional lines needed due to wrapping for the given line.
///
/// Does not account for any potential linebreaks in `line`
///
/// If `line` fits in `terminal_columns` returns 0
fn estimated_wrapped_line_count(line: &str, terminal_columns: u16) -> usize {
    let estimated_width = line_width(line);
    let terminal_columns: usize = terminal_columns.into();

    // integer ceiling rounding division for positive divisors
    let estimated_line_count = (estimated_width + terminal_columns - 1) / terminal_columns;

    // Any wrapping will add to our overall line count
    estimated_line_count.saturating_sub(1)
}

/// Compute the line width for ANSI escaped text
fn line_width(line: &str) -> usize {
    strip_ansi(line).width()
}

// Returns a string that skips N number of lines with the next offset of lines
// An offset of 0 would return only one line after skipping the required lines
fn skip_buffer_lines(string: &str, skip: usize, offset: Option<usize>) -> &str {
    let mut matches = string.match_indices('\n');
    let index = if skip == 0 {
        0
    } else {
        matches
            .clone()
            .nth(skip - 1)
            .map(|(index, _)| index + 1)
            .unwrap_or(string.len())
    };

    let limit = match offset {
        Some(offset) => {
            let offset = skip + offset;
            matches
                .nth(offset)
                .map(|(index, _)| index)
                .unwrap_or(string.len())
        }
        None => string.len(),
    };

    string[index..limit].trim_end_matches('\n')
}

fn coerce_crlf(input: &str) -> Cow<str> {
    let mut result = Cow::Borrowed(input);
    let mut cursor: usize = 0;
    for (idx, _) in input.match_indices('\n') {
        if idx > 0 && input.as_bytes()[idx - 1] == b'\r' {
            // Already CRLF: Don't advance cursor as a pure LF could follow
        } else {
            if let Cow::Borrowed(_) = result {
                result = Cow::Owned(String::with_capacity(input.len()));
            }
            result += &input[cursor..idx];
            result += "\r\n";
            cursor = idx + 1;
        }
    }
    if let Cow::Owned(_) = result {
        result += &input[cursor..input.len()]
    }
    result
}

/// the type used by crossterm operations
pub type W = std::io::BufWriter<std::io::Stderr>;

pub struct Painter {
    // Stdout
    stdout: W,
    prompt_coords: PromptCoordinates,
    terminal_size: (u16, u16),
    last_required_lines: u16,
    pub large_buffer: bool,
    debug_mode: bool,
}

impl Painter {
    pub fn new(stdout: W) -> Self {
        Painter {
            stdout,
            prompt_coords: PromptCoordinates::default(),
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            debug_mode: false,
        }
    }

    pub fn new_with_debug(stdout: W) -> Self {
        Painter {
            stdout,
            prompt_coords: PromptCoordinates::default(),
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            debug_mode: true,
        }
    }

    /// Update the terminal size information by polling the system
    pub(crate) fn init_terminal_size(&mut self) -> Result<()> {
        self.terminal_size = terminal::size()?;
        Ok(())
    }

    fn terminal_rows(&self) -> u16 {
        self.terminal_size.1
    }

    pub fn terminal_cols(&self) -> u16 {
        self.terminal_size.0
    }

    pub fn remaining_lines(&self) -> u16 {
        self.terminal_size.1 - self.prompt_coords.prompt_start.1
    }

    /// Sets the prompt origin position.
    pub(crate) fn initialize_prompt_position(&mut self) -> Result<()> {
        // Cursor positions are 0 based here.
        let (column, row) = cursor::position()?;
        // Assumption: if the cursor is not on the zeroth column,
        // there is content we want to leave intact, thus advance to the next row
        let new_row = if column > 0 { row + 1 } else { row };
        // TODO: support more than just two line prompts!
        //  If we are on the last line or would move beyond the last line due to
        //  the condition above, we need to make room for the multiline prompt.
        //  Otherwise printing the prompt would scroll of the stored prompt
        //  origin, causing issues after repaints.
        let new_row = if new_row == self.terminal_rows() {
            // Would exceed the terminal height, we need space for two lines
            self.print_crlf()?;
            self.print_crlf()?;
            new_row.saturating_sub(2)
        } else if new_row == self.terminal_rows() - 1 {
            // Safe on the last line make space for the 2 line prompt
            self.print_crlf()?;
            new_row.saturating_sub(1)
        } else {
            new_row
        };
        self.prompt_coords.set_prompt_start(0, new_row);
        Ok(())
    }

    /// Main pain painter for the prompt and buffer
    /// It queues all the actions required to print the prompt together with
    /// lines that make the buffer.
    /// Using the prompt lines object in this function it is estimated how the
    /// prompt should scroll up and how much space is required to print all the
    /// lines for the buffer
    ///
    /// Note. The ScrollUp operation in crossterm deletes lines from the top of
    /// the screen.
    pub fn repaint_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: PromptLines,
        menu: Option<&dyn Menu>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        self.stdout.queue(cursor::Hide)?;

        // String representation of the prompt
        let (screen_width, screen_height) = self.terminal_size;

        // Lines and distance parameters
        let required_lines = lines.required_lines(screen_width, menu);
        let remaining_lines = self.remaining_lines();

        // Marking the painter state as larger buffer to avoid animations
        self.large_buffer = required_lines >= screen_height;

        // Moving the start position of the cursor based on the size of the required lines
        if self.large_buffer {
            self.prompt_coords.prompt_start.1 = 0;
        } else if required_lines >= remaining_lines {
            let extra = required_lines.saturating_sub(remaining_lines);
            self.stdout.queue(ScrollUp(extra))?;
            self.prompt_coords.prompt_start.1 =
                self.prompt_coords.prompt_start.1.saturating_sub(extra);
        }

        // Moving the cursor to the start of the prompt
        // from this position everything will be printed
        self.stdout
            .queue(cursor::MoveTo(0, self.prompt_coords.prompt_start.1))?
            .queue(Clear(ClearType::FromCursorDown))?;

        if self.large_buffer {
            self.print_large_buffer(prompt, &lines, menu, use_ansi_coloring)?
        } else {
            self.print_small_buffer(prompt, &lines, menu, use_ansi_coloring)?
        }

        // The last_required_lines is used to move the cursor at the end where stdout
        // can print without overwriting the things written during the painting
        self.last_required_lines = required_lines;

        // In debug mode a string with position information is printed at the end of the buffer
        if self.debug_mode {
            let cursor_distance = lines.distance_from_prompt(screen_width);
            let prompt_lines = lines.prompt_lines_with_wrap(screen_width);
            let prompt_length = lines.prompt_str_left.len() + lines.prompt_indicator.len();
            let estimated_prompt =
                estimated_wrapped_line_count(&lines.prompt_str_left, screen_width);

            self.stdout
                .queue(Print(format!(" [h{}:", screen_height)))?
                .queue(Print(format!("w{}] ", screen_width)))?
                .queue(Print(format!("[x{}:", self.prompt_coords.prompt_start.0)))?
                .queue(Print(format!("y{}] ", self.prompt_coords.prompt_start.1)))?
                .queue(Print(format!("rm:{} ", remaining_lines)))?
                .queue(Print(format!("re:{} ", required_lines)))?
                .queue(Print(format!("di:{} ", cursor_distance)))?
                .queue(Print(format!("pl:{} ", prompt_lines)))?
                .queue(Print(format!("pr:{} ", prompt_length)))?
                .queue(Print(format!("wr:{} ", estimated_prompt)))?
                .queue(Print(format!("ls:{} ", self.last_required_lines)))?;
        }

        self.stdout.queue(RestorePosition)?.queue(cursor::Show)?;

        self.stdout.flush()
    }

    fn print_right_prompt(&mut self, lines: &PromptLines) -> Result<()> {
        let (screen_width, _) = self.terminal_size;
        let prompt_length_right = line_width(&lines.prompt_str_right);
        let start_position = screen_width.saturating_sub(prompt_length_right as u16);
        let input_width = lines.estimate_first_input_line_width();

        if input_width <= start_position {
            self.stdout
                .queue(SavePosition)?
                .queue(cursor::MoveTo(
                    start_position,
                    self.prompt_coords.prompt_start.1,
                ))?
                .queue(Print(&coerce_crlf(&lines.prompt_str_right)))?
                .queue(RestorePosition)?;
        }

        Ok(())
    }

    fn print_menu(
        &mut self,
        menu: &dyn Menu,
        lines: &PromptLines,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        let (screen_width, screen_height) = self.terminal_size;
        let cursor_distance = lines.distance_from_prompt(screen_width);

        // If there is not enough space to print the menu, then the starting
        // drawing point for the menu will overwrite the last rows in the buffer
        let starting_row = if cursor_distance >= screen_height.saturating_sub(1) {
            screen_height.saturating_sub(menu.min_rows())
        } else {
            self.prompt_coords.prompt_start.1 + cursor_distance + 1
        };

        let remaining_lines = screen_height.saturating_sub(starting_row);
        let menu_string = menu.menu_string(remaining_lines, use_ansi_coloring);

        self.stdout
            .queue(cursor::MoveTo(0, starting_row))?
            .queue(Clear(ClearType::FromCursorDown))?
            .queue(Print(menu_string.trim_end_matches('\n')))?;

        Ok(())
    }

    fn print_small_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        menu: Option<&dyn Menu>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        // print our prompt with color
        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_color()))?;
        }

        self.stdout
            .queue(Print(&coerce_crlf(&lines.prompt_str_left)))?;

        let prompt_indicator = match menu {
            Some(menu) => menu.indicator(),
            None => &lines.prompt_indicator,
        };
        self.stdout.queue(Print(&coerce_crlf(prompt_indicator)))?;

        self.print_right_prompt(lines)?;

        if use_ansi_coloring {
            self.stdout.queue(ResetColor)?;
        }

        self.stdout
            .queue(Print(&lines.before_cursor))?
            .queue(SavePosition)?
            .queue(Print(&lines.after_cursor))?;

        if let Some(menu) = menu {
            self.print_menu(menu, lines, use_ansi_coloring)?;
        } else {
            self.stdout.queue(Print(&lines.hint))?;
        }

        Ok(())
    }

    fn print_large_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        menu: Option<&dyn Menu>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        let (screen_width, screen_height) = self.terminal_size;
        let cursor_distance = lines.distance_from_prompt(screen_width);
        let remaining_lines = screen_height.saturating_sub(cursor_distance);

        // Calculating the total lines before the cursor
        // The -1 in the total_lines_before is there because the at least one line of the prompt
        // indicator is printed in the same line as the first line of the buffer
        let prompt_lines = lines.prompt_lines_with_wrap(screen_width) as usize;

        let prompt_indicator = match menu {
            Some(menu) => menu.indicator(),
            None => &lines.prompt_indicator,
        };

        let prompt_indicator_lines = prompt_indicator.lines().count();
        let before_cursor_lines = lines.before_cursor.lines().count();
        let total_lines_before = prompt_lines + prompt_indicator_lines + before_cursor_lines - 1;

        // Extra rows represent how many rows are "above" the visible area in the terminal
        let extra_rows = (total_lines_before).saturating_sub(screen_height as usize);

        // print our prompt with color
        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_color()))?;
        }

        // In case the prompt is made out of multiple lines, the prompt is split by
        // lines and only the required ones are printed
        let prompt_skipped = skip_buffer_lines(&lines.prompt_str_left, extra_rows, None);
        self.stdout.queue(Print(&coerce_crlf(prompt_skipped)))?;

        if extra_rows == 0 {
            self.print_right_prompt(lines)?;
        }

        // Adjusting extra_rows base on the calculated prompt line size
        let extra_rows = extra_rows.saturating_sub(prompt_lines);

        let indicator_skipped = skip_buffer_lines(prompt_indicator, extra_rows, None);
        self.stdout.queue(Print(&coerce_crlf(indicator_skipped)))?;

        if use_ansi_coloring {
            self.stdout.queue(ResetColor)?;
        }

        // The minimum number of lines from the menu are removed from the buffer if there is no more
        // space to print the menu. This will only happen if the cursor is at the last line and
        // it is a large buffer
        let offset = menu.and_then(|menu| {
            if cursor_distance >= screen_height.saturating_sub(1) {
                let rows = lines
                    .before_cursor
                    .lines()
                    .count()
                    .saturating_sub(extra_rows)
                    .saturating_sub(menu.min_rows() as usize);
                Some(rows)
            } else {
                None
            }
        });

        // Selecting the lines before the cursor that will be printed
        let before_cursor_skipped = skip_buffer_lines(&lines.before_cursor, extra_rows, offset);
        self.stdout.queue(Print(before_cursor_skipped))?;
        self.stdout.queue(SavePosition)?;

        if let Some(menu) = menu {
            // TODO: Also solve the difficult problem of displaying (parts of)
            // the content after the cursor with the completion menu
            self.print_menu(menu, lines, use_ansi_coloring)?;
        } else {
            // Selecting lines for the hint
            // The -1 subtraction is done because the remaining lines consider the line where the
            // cursor is located as a remaining line. That has to be removed to get the correct offset
            // for the after-cursor and hint lines
            let offset = remaining_lines.saturating_sub(1) as usize;
            // Selecting lines after the cursor
            let after_cursor_skipped = skip_buffer_lines(&lines.after_cursor, 0, Some(offset));
            self.stdout.queue(Print(after_cursor_skipped))?;
            // Hint lines
            let hint_skipped = skip_buffer_lines(&lines.hint, 0, Some(offset));
            self.stdout.queue(Print(hint_skipped))?;
        }

        Ok(())
    }

    /// Updates prompt origin and offset to handle a screen resize event
    pub(crate) fn handle_resize(&mut self, width: u16, height: u16) {
        let prev_terminal_size = self.terminal_size;

        self.terminal_size = (width, height);
        // TODO properly adjusting prompt_origin on resizing while lines > 1

        let current_origin = self.prompt_coords.prompt_start;

        if current_origin.1 >= (height - 1) {
            // Terminal is shrinking up
            // FIXME: use actual prompt size at some point
            // Note: you can't just subtract the offset from the origin,
            // as we could be shrinking so fast that the offset we read back from
            // crossterm is past where it would have been.
            self.prompt_coords
                .set_prompt_start(current_origin.0, height - 2);
        } else if prev_terminal_size.1 < height {
            // Terminal is growing down, so move the prompt down the same amount to make space
            // for history that's on the screen
            // Note: if the terminal doesn't have sufficient history, this will leave a trail
            // of previous prompts currently.
            self.prompt_coords.set_prompt_start(
                current_origin.0,
                current_origin.1 + (height - prev_terminal_size.1),
            );
        }
    }

    /// Writes `line` to the terminal with a following carriage return and newline
    pub(crate) fn paint_line(&mut self, line: &str) -> Result<()> {
        self.stdout.queue(Print(line))?.queue(Print("\r\n"))?;

        self.stdout.flush()
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub(crate) fn print_crlf(&mut self) -> Result<()> {
        self.stdout.queue(Print("\r\n"))?;

        self.stdout.flush()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.stdout.queue(cursor::Hide)?;
        let (_, num_lines) = terminal::size()?;
        for _ in 0..2 * num_lines {
            self.stdout.queue(Print("\n"))?;
        }
        self.stdout.queue(MoveTo(0, 0))?;
        self.stdout.queue(cursor::Show)?;

        self.stdout.flush()
    }

    // The prompt is moved to the end of the buffer after the event was handled
    // If the prompt is in the middle of a multiline buffer, then the output to stdout
    // could overwrite the buffer writing
    pub fn move_cursor_to_end(&mut self) -> Result<()> {
        let final_row = self.prompt_coords.prompt_start.1 + self.last_required_lines;
        let scroll = final_row.saturating_sub(self.terminal_rows() - 1);
        if scroll != 0 {
            self.stdout.queue(ScrollUp(scroll))?;
        }
        self.stdout
            .queue(MoveTo(0, final_row.min(self.terminal_rows() - 1)))?;

        self.stdout.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[test]
    fn test_skip_lines() {
        let string = "sentence1\nsentence2\nsentence3\n";

        assert_eq!(skip_buffer_lines(string, 1, None), "sentence2\nsentence3");
        assert_eq!(skip_buffer_lines(string, 2, None), "sentence3");
        assert_eq!(skip_buffer_lines(string, 3, None), "");
        assert_eq!(skip_buffer_lines(string, 4, None), "");
    }

    #[test]
    fn test_skip_lines_no_newline() {
        let string = "sentence1";

        assert_eq!(skip_buffer_lines(string, 0, None), "sentence1");
        assert_eq!(skip_buffer_lines(string, 1, None), "");
    }

    #[test]
    fn test_skip_lines_with_limit() {
        let string = "sentence1\nsentence2\nsentence3\nsentence4\nsentence5";

        assert_eq!(
            skip_buffer_lines(string, 1, Some(1)),
            "sentence2\nsentence3",
        );

        assert_eq!(
            skip_buffer_lines(string, 1, Some(2)),
            "sentence2\nsentence3\nsentence4",
        );

        assert_eq!(
            skip_buffer_lines(string, 2, Some(1)),
            "sentence3\nsentence4",
        );

        assert_eq!(
            skip_buffer_lines(string, 1, Some(10)),
            "sentence2\nsentence3\nsentence4\nsentence5",
        );

        assert_eq!(
            skip_buffer_lines(string, 0, Some(1)),
            "sentence1\nsentence2",
        );

        assert_eq!(skip_buffer_lines(string, 0, Some(0)), "sentence1",);
        assert_eq!(skip_buffer_lines(string, 1, Some(0)), "sentence2",);
    }

    #[rstest]
    #[case("sentence\nsentence", "sentence\r\nsentence")]
    #[case("sentence\r\nsentence", "sentence\r\nsentence")]
    #[case("sentence\nsentence\n", "sentence\r\nsentence\r\n")]
    #[case("😇\nsentence", "😇\r\nsentence")]
    #[case("sentence\n😇", "sentence\r\n😇")]
    #[case("\n", "\r\n")]
    #[case("", "")]
    fn test_coerce_crlf(#[case] input: &str, #[case] expected: &str) {
        let result = coerce_crlf(input);

        assert_eq!(result, expected);

        assert!(
            input != expected || matches!(result, Cow::Borrowed(_)),
            "Unnecessary allocation"
        )
    }
}
