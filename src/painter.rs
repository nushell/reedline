use crate::PromptHistorySearch;
use crossterm::{cursor::MoveToRow, style::ResetColor, terminal::ScrollUp};

use {
    crate::{prompt::PromptEditMode, Prompt},
    crossterm::{
        cursor::{self, MoveTo, MoveToColumn, RestorePosition, SavePosition},
        style::{Print, SetForegroundColor},
        terminal::{self, Clear, ClearType},
        QueueableCommand, Result,
    },
    std::io::{Stdout, Write},
    unicode_width::UnicodeWidthStr,
};

// const END_LINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

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
    before_cursor: &'prompt str,
    after_cursor: &'prompt str,
    hint: &'prompt str,
}

impl<'prompt> PromptLines<'prompt> {
    /// Splits the strings before and after the cursor as well as the hint
    /// This vector with the str are used to calculate how many lines are
    /// required to print after the prompt
    pub fn new(
        before_cursor: &'prompt str,
        after_cursor: &'prompt str,
        hint: &'prompt str,
    ) -> Self {
        Self {
            before_cursor,
            after_cursor,
            hint,
        }
    }

    /// The required lines to paint the buffer are calculated by counting the
    /// number of newlines in all the strings that form the prompt and buffer.
    /// The plus 1 is to indicate that there should be at least one line.
    fn required_lines(
        &self,
        prompt_str: &str,
        prompt_indicator: &str,
        terminal_columns: u16,
    ) -> u16 {
        let mut lines = prompt_str.matches('\n').count()
            + prompt_indicator.matches('\n').count()
            + self.before_cursor.matches('\n').count()
            + self.hint.matches('\n').count()
            + self.after_cursor.matches('\n').count()
            + 1;

        // adjust lines by the number of wrapped additional lines we have
        let input =
            prompt_indicator.to_string() + self.before_cursor + self.after_cursor + self.hint;
        for line in input.split('\n') {
            if let Ok(line) = strip_ansi_escapes::strip(line) {
                lines +=
                    estimated_wrapped_line_count(&String::from_utf8_lossy(&line), terminal_columns)
            } else {
                lines += estimated_wrapped_line_count(line, terminal_columns)
            }
        }

        lines as u16
    }
}

fn estimated_wrapped_line_count(line: &str, terminal_columns: u16) -> usize {
    let estimated_width = UnicodeWidthStr::width(line);

    let estimated_line_count = estimated_width as f64 / terminal_columns as f64;
    let estimated_line_count = estimated_line_count.ceil() as u64;

    // Any wrapping will add to our overall line count
    if estimated_line_count >= 1 {
        estimated_line_count as usize - 1
    } else {
        0 // no wrapping
    }
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

    let line = &string[index..limit];

    line.trim_end_matches('\n')
}

pub struct Painter {
    // Stdout
    stdout: Stdout,
    prompt_coords: PromptCoordinates,
    terminal_size: (u16, u16),
    last_required_lines: u16,
    pub large_buffer: bool,
    debug_mode: bool,
}

impl Painter {
    pub fn new(stdout: Stdout) -> Self {
        Painter {
            stdout,
            prompt_coords: PromptCoordinates::default(),
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            debug_mode: false,
        }
    }

    pub fn new_with_debug(stdout: Stdout) -> Self {
        Painter {
            stdout,
            prompt_coords: PromptCoordinates::default(),
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            debug_mode: true,
        }
    }

    /// Calculates the distance from the prompt
    pub fn distance_from_prompt(&self) -> Result<u16> {
        let (_, cursor_row) = cursor::position()?;
        let distance_from_prompt = cursor_row.saturating_sub(self.prompt_coords.prompt_start.1);

        Ok(distance_from_prompt)
    }

    /// Update the terminal size information by polling the system
    pub(crate) fn init_terminal_size(&mut self) -> Result<()> {
        self.terminal_size = terminal::size()?;
        Ok(())
    }

    fn terminal_rows(&self) -> u16 {
        self.terminal_size.1
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
        prompt_mode: PromptEditMode,
        lines: PromptLines,
        history_indicator: Option<PromptHistorySearch>,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        self.stdout.queue(cursor::Hide)?;

        // String representation of the prompt
        let (screen_width, screen_height) = self.terminal_size;
        let prompt_str = prompt.render_prompt(screen_width as usize);

        // The prompt indicator could be normal one or the history indicator
        let prompt_indicator = match history_indicator {
            Some(prompt_search) => prompt.render_prompt_history_search_indicator(prompt_search),
            None => prompt.render_prompt_indicator(prompt_mode),
        };

        // Lines and distance parameters
        let required_lines = lines.required_lines(&prompt_str, &prompt_indicator, screen_width);
        let remaining_lines = self.remaining_lines();

        // Marking the painter state as larger buffer to avoid animations
        self.large_buffer = required_lines >= screen_height;

        // Cursor distance from prompt
        let cursor_distance = self.distance_from_prompt()?;

        // Delta indicates how many row are required based on the distance
        // from the prompt. The closer the cursor to the prompt (smaller distance)
        // the larger the delta and the real required extra lines
        let delta = required_lines.saturating_sub(cursor_distance);

        // Moving the start position of the cursor based on the size of the required lines
        if self.large_buffer {
            self.prompt_coords.prompt_start.1 = 0;
        } else if delta > remaining_lines {
            // Checked sub in case there is overflow
            let sub = self
                .prompt_coords
                .prompt_start
                .1
                .saturating_sub(required_lines);

            let prompt_size = prompt_str.lines().count() as u16;
            self.stdout.queue(ScrollUp(delta))?;
            self.prompt_coords.prompt_start.1 = sub + prompt_size;
        } else if (required_lines > cursor_distance
            && remaining_lines.saturating_sub(cursor_distance) <= 1)
            || required_lines == remaining_lines
        {
            // Conditions to scroll one row up and update prompt position:
            // - If the required lines is larger than the cursor distance
            //      then it means that the cursor is at the bottom of the screen and
            //      we need to scroll one row up (this condition applies when
            //      at the bottom of a multi line buffer)
            // - If the number of required lines is equal to the number of
            //      remaining lines (this condition applies when inserting text
            //      in the middle of a multi line buffer)
            self.stdout.queue(ScrollUp(1))?;
            self.prompt_coords.prompt_start.1 = self.prompt_coords.prompt_start.1.saturating_sub(1);
        }

        // Moving the cursor to the start of the prompt
        // from this position everything will be printed
        self.stdout.queue(cursor::MoveTo(
            self.prompt_coords.prompt_start.0,
            self.prompt_coords.prompt_start.1,
        ))?;

        self.stdout
            .queue(MoveToColumn(0))?
            .queue(Clear(ClearType::FromCursorDown))?;

        if self.large_buffer {
            self.print_large_buffer(
                prompt,
                (&prompt_str, &prompt_indicator),
                lines,
                cursor_distance,
                use_ansi_coloring,
            )?
        } else {
            self.print_small_buffer(
                prompt,
                &prompt_str,
                &prompt_indicator,
                lines,
                use_ansi_coloring,
            )?
        }

        // The last_required_lines is used to move the cursor at the end where stdout
        // can print without overwriting the things written during the paining
        self.last_required_lines = required_lines + prompt_str.lines().count() as u16;

        // In debug mode a string with position information is printed at the end of the buffer
        if self.debug_mode {
            let prompt_length = prompt_str.len() + prompt_indicator.len();
            let estimated_prompt = estimated_wrapped_line_count(&prompt_str, screen_width);

            self.stdout
                .queue(Print(format!(" [h{}:", screen_height)))?
                .queue(Print(format!("w{}] ", screen_width)))?
                .queue(Print(format!("[x{}:", self.prompt_coords.prompt_start.0)))?
                .queue(Print(format!("y{}] ", self.prompt_coords.prompt_start.1)))?
                .queue(Print(format!("re:{} ", required_lines)))?
                .queue(Print(format!("de:{} ", delta)))?
                .queue(Print(format!("di:{} ", cursor_distance)))?
                .queue(Print(format!("pr:{} ", prompt_length)))?
                .queue(Print(format!("wr:{} ", estimated_prompt)))?
                .queue(Print(format!("rm:{} ", remaining_lines)))?
                .queue(Print(format!("ls:{} ", self.last_required_lines)))?;
        }

        self.stdout.queue(RestorePosition)?.queue(cursor::Show)?;

        self.flush()
    }

    fn print_small_buffer(
        &mut self,
        prompt: &dyn Prompt,
        prompt_str: &str,
        prompt_indicator: &str,
        lines: PromptLines,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        // print our prompt with color
        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_color()))?;
        }

        self.stdout
            .queue(Print(&prompt_str))?
            .queue(Print(&prompt_indicator))?;

        if use_ansi_coloring {
            self.stdout.queue(ResetColor)?;
        }

        self.stdout
            .queue(Print(&lines.before_cursor))?
            .queue(SavePosition)?
            .queue(Print(&lines.hint))?
            .queue(Print(&lines.after_cursor))?;

        Ok(())
    }

    fn print_large_buffer(
        &mut self,
        prompt: &dyn Prompt,
        prompt_str: (&str, &str),
        lines: PromptLines,
        cursor_distance: u16,
        use_ansi_coloring: bool,
    ) -> Result<()> {
        let (prompt_str, prompt_indicator) = prompt_str;
        let (screen_width, screen_height) = self.terminal_size;
        let remaining_lines = screen_height.saturating_sub(cursor_distance);

        // Calculating the total lines before the cursor
        // The -1 in the total_lines_before is there because the at least one line of the prompt
        // indicator is printed in the same line as the first line of the buffer
        let complete_prompt = prompt_str.to_string() + prompt_indicator;
        let prompt_wrap = estimated_wrapped_line_count(&complete_prompt, screen_width);
        let prompt_lines = prompt_str.matches('\n').count() + prompt_wrap;

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
        let prompt_skipped = skip_buffer_lines(prompt_str, extra_rows, None);
        self.stdout.queue(Print(prompt_skipped))?;

        // Adjusting extra_rows base on the calculated prompt line size
        let extra_rows = extra_rows.saturating_sub(prompt_lines);

        let indicator_skipped = skip_buffer_lines(prompt_indicator, extra_rows, None);
        self.stdout.queue(Print(indicator_skipped))?;

        if use_ansi_coloring {
            self.stdout.queue(ResetColor)?;
        }

        // Selecting the lines before the cursor that will be printed
        let before_cursor_skipped = skip_buffer_lines(lines.before_cursor, extra_rows, None);
        self.stdout.queue(Print(before_cursor_skipped))?;
        self.stdout.queue(SavePosition)?;

        // Selecting lines for the hint
        // The -1 subtraction is done because the remaining lines consider the line where the
        // cursor is located as a remaining line. That has to be removed to get the correct offset
        // for the hint and after cursor lines
        let offset = remaining_lines.saturating_sub(1) as usize;
        let hint_skipped = skip_buffer_lines(lines.hint, 0, Some(offset));
        self.stdout.queue(Print(hint_skipped))?;

        // Selecting lines after the cursor
        let after_cursor_skipped = skip_buffer_lines(lines.after_cursor, 0, Some(offset));
        self.stdout.queue(Print(after_cursor_skipped))?;

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
    pub fn paint_line(&mut self, line: &str) -> Result<()> {
        self.stdout
            .queue(Print(line))?
            .queue(Print("\n"))?
            .queue(MoveToColumn(1))?;

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
        let (_, num_lines) = terminal::size()?;
        for _ in 0..2 * num_lines {
            self.stdout.queue(Print("\n"))?;
        }
        self.stdout.queue(MoveTo(0, 0))?;

        self.stdout.flush()
    }

    // The prompt is moved to the end of the buffer after the event was handled
    // If the prompt is in the middle of a multiline buffer, then the output to stdout
    // could overwrite the buffer writing
    pub fn move_cursor_to_end(&mut self) -> Result<()> {
        let final_row = self.prompt_coords.prompt_start.1 + self.last_required_lines;
        self.stdout.queue(MoveToRow(final_row))?;

        self.stdout.flush()
    }

    pub fn flush(&mut self) -> Result<()> {
        self.stdout.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
