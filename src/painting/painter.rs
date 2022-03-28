use {
    super::utils::{coerce_crlf, line_width},
    crate::{
        menu::{Menu, ReedlineMenu},
        painting::PromptLines,
        Prompt,
    },
    crossterm::{
        cursor::{self, MoveTo, RestorePosition, SavePosition},
        style::{Print, ResetColor, SetForegroundColor},
        terminal::{self, Clear, ClearType, ScrollUp},
        QueueableCommand, Result,
    },
    std::io::Write,
};

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

/// the type used by crossterm operations
pub type W = std::io::BufWriter<std::io::Stderr>;

/// Implementation of the output to the terminal
pub struct Painter {
    // Stdout
    stdout: W,
    prompt_start_row: u16,
    terminal_size: (u16, u16),
    last_required_lines: u16,
    large_buffer: bool,
}

impl Painter {
    pub(crate) fn new(stdout: W) -> Self {
        Painter {
            stdout,
            prompt_start_row: 0,
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
        }
    }

    /// Height of the current terminal window
    pub fn screen_height(&self) -> u16 {
        self.terminal_size.1
    }

    /// Width of the current terminal window
    pub fn screen_width(&self) -> u16 {
        self.terminal_size.0
    }

    /// Returns the available lines from the prompt down
    pub fn remaining_lines(&self) -> u16 {
        self.screen_height() - self.prompt_start_row
    }

    /// Check if the currently painted content exceeds the size of the screen
    /// and thus should not be repainted without reason (disable animation
    /// repaint)
    pub(crate) fn exceeds_screen_size(&self) -> bool {
        self.large_buffer
    }

    /// Sets the prompt origin position and screen size for a new line editor
    /// invocation
    ///
    /// Not to be used for resizes during a running line editor, use
    /// [`Painter::handle_resize()`] instead
    pub(crate) fn initialize_prompt_position(&mut self) -> Result<()> {
        // Update the terminal size
        self.terminal_size = terminal::size()?;
        // Cursor positions are 0 based here.
        let (column, row) = cursor::position()?;
        // Assumption: if the cursor is not on the zeroth column,
        // there is content we want to leave intact, thus advance to the next row
        let new_row = if column > 0 { row + 1 } else { row };
        //  If we are on the last line and would move beyond the last line due to
        //  the condition above, we need to make room for the prompt.
        //  Otherwise printing the prompt would scroll of the stored prompt
        //  origin, causing issues after repaints.
        let new_row = if new_row == self.screen_height() {
            self.print_crlf()?;
            new_row.saturating_sub(1)
        } else {
            new_row
        };
        self.prompt_start_row = new_row;
        Ok(())
    }

    /// Main pain painter for the prompt and buffer
    /// It queues all the actions required to print the prompt together with
    /// lines that make the buffer.
    /// Using the prompt lines object in this function it is estimated how the
    /// prompt should scroll up and how much space is required to print all the
    /// lines for the buffer
    ///
    /// Note. The `ScrollUp` operation in `crossterm` deletes lines from the top of
    /// the screen.
    pub(crate) fn repaint_buffer(
        &mut self,
        prompt: &dyn Prompt,
        lines: &PromptLines,
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        self.stdout.queue(cursor::Hide)?;

        let screen_width = self.screen_width();
        let screen_height = self.screen_height();

        // Lines and distance parameters
        let remaining_lines = self.remaining_lines();
        let required_lines = lines.required_lines(screen_width, menu);

        // Marking the painter state as larger buffer to avoid animations
        self.large_buffer = required_lines >= screen_height;

        // Moving the start position of the cursor based on the size of the required lines
        if self.large_buffer {
            self.prompt_start_row = 0;
        } else if required_lines >= remaining_lines {
            let extra = required_lines.saturating_sub(remaining_lines);
            self.stdout.queue(ScrollUp(extra))?;
            self.prompt_start_row = self.prompt_start_row.saturating_sub(extra);
        }

        // Moving the cursor to the start of the prompt
        // from this position everything will be printed
        self.stdout
            .queue(cursor::MoveTo(0, self.prompt_start_row))?
            .queue(Clear(ClearType::FromCursorDown))?;

        if self.large_buffer {
            self.print_large_buffer(prompt, lines, menu, use_ansi_coloring)?;
        } else {
            self.print_small_buffer(prompt, lines, menu, use_ansi_coloring)?;
        }

        // The last_required_lines is used to move the cursor at the end where stdout
        // can print without overwriting the things written during the painting
        self.last_required_lines = required_lines;

        self.stdout.queue(RestorePosition)?.queue(cursor::Show)?;

        self.stdout.flush()
    }

    fn print_right_prompt(&mut self, lines: &PromptLines) -> Result<()> {
        let prompt_length_right = line_width(&lines.prompt_str_right);
        let start_position = self
            .screen_width()
            .saturating_sub(prompt_length_right as u16);
        let input_width = lines.estimate_first_input_line_width();

        if input_width <= start_position {
            self.stdout
                .queue(SavePosition)?
                .queue(cursor::MoveTo(start_position, self.prompt_start_row))?
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
        let screen_width = self.screen_width();
        let screen_height = self.screen_height();
        let cursor_distance = lines.distance_from_prompt(screen_width);

        // If there is not enough space to print the menu, then the starting
        // drawing point for the menu will overwrite the last rows in the buffer
        let starting_row = if cursor_distance >= screen_height.saturating_sub(1) {
            screen_height.saturating_sub(menu.min_rows())
        } else {
            self.prompt_start_row + cursor_distance + 1
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
        menu: Option<&ReedlineMenu>,
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
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        let screen_width = self.screen_width();
        let screen_height = self.screen_height();
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
        let prev_prompt_row = self.prompt_start_row;

        self.terminal_size = (width, height);
        // TODO properly adjusting prompt_origin on resizing while lines > 1

        if prev_prompt_row >= (height - 1) {
            // Terminal is shrinking up
            // FIXME: use actual prompt size at some point
            // Note: you can't just subtract the offset from the origin,
            // as we could be shrinking so fast that the offset we read back from
            // crossterm is past where it would have been.
            self.prompt_start_row = height - 2;
        } else if prev_terminal_size.1 < height {
            // Terminal is growing down, so move the prompt down the same amount to make space
            // for history that's on the screen
            // Note: if the terminal doesn't have sufficient history, this will leave a trail
            // of previous prompts currently.
            self.prompt_start_row = prev_prompt_row + (height - prev_terminal_size.1);
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
    pub(crate) fn clear_screen(&mut self) -> Result<()> {
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
    pub(crate) fn move_cursor_to_end(&mut self) -> Result<()> {
        let final_row = self.prompt_start_row + self.last_required_lines;
        let scroll = final_row.saturating_sub(self.screen_height() - 1);
        if scroll != 0 {
            self.stdout.queue(ScrollUp(scroll))?;
        }
        self.stdout
            .queue(MoveTo(0, final_row.min(self.screen_height() - 1)))?;

        self.stdout.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
}
