use crate::{CursorConfig, PromptEditMode, PromptViMode};

use {
    super::utils::{coerce_crlf, line_width},
    crate::{
        menu::{Menu, ReedlineMenu},
        painting::PromptLines,
        Prompt,
    },
    crossterm::{
        cursor::{self, MoveTo, RestorePosition, SavePosition},
        style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor},
        terminal::{self, Clear, ClearType},
        QueueableCommand,
    },
    std::io::{Result, Write},
    std::ops::RangeInclusive,
};
#[cfg(feature = "external_printer")]
use {crate::LineBuffer, crossterm::cursor::MoveUp};

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

#[derive(Debug, PartialEq, Eq)]
pub struct PainterSuspendedState {
    previous_prompt_rows_range: RangeInclusive<u16>,
}

#[derive(Debug, PartialEq, Eq)]
enum PromptRowSelector {
    UseExistingPrompt { start_row: u16 },
    MakeNewPrompt { new_row: u16 },
}

// Selects the row where the next prompt should start on, taking into account and whether it should re-use a previous
// prompt.
fn select_prompt_row(
    suspended_state: Option<&PainterSuspendedState>,
    (column, row): (u16, u16), // NOTE: Positions are 0 based here
) -> PromptRowSelector {
    if let Some(painter_state) = suspended_state {
        // The painter was suspended, try to re-use the last prompt position to avoid
        // unnecessarily making new prompts.
        if painter_state.previous_prompt_rows_range.contains(&row) {
            // Cursor is still in the range of the previous prompt, re-use it.
            let start_row = *painter_state.previous_prompt_rows_range.start();
            return PromptRowSelector::UseExistingPrompt { start_row };
        } else {
            // There was some output or cursor is outside of the range of previous prompt make a
            // fresh new prompt.
        }
    }

    // Assumption: if the cursor is not on the zeroth column,
    //   there is content we want to leave intact, thus advance to the next row.
    let new_row = if column > 0 { row + 1 } else { row };
    PromptRowSelector::MakeNewPrompt { new_row }
}

/// Implementation of the output to the terminal
pub struct Painter {
    // Stdout
    stdout: W,
    prompt_start_row: u16,
    terminal_size: (u16, u16),
    last_required_lines: u16,
    large_buffer: bool,
    just_resized: bool,
    after_cursor_lines: Option<String>,
}

impl Painter {
    pub(crate) fn new(stdout: W) -> Self {
        Painter {
            stdout,
            prompt_start_row: 0,
            terminal_size: (0, 0),
            last_required_lines: 0,
            large_buffer: false,
            just_resized: false,
            after_cursor_lines: None,
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
        self.screen_height().saturating_sub(self.prompt_start_row)
    }

    /// Returns the state necessary before suspending the painter (to run a host command event).
    ///
    /// This state will be used to re-initialize the painter to re-use last prompt if possible.
    pub fn state_before_suspension(&self) -> PainterSuspendedState {
        let start_row = self.prompt_start_row;
        let final_row = start_row + self.last_required_lines;
        PainterSuspendedState {
            previous_prompt_rows_range: start_row..=final_row,
        }
    }

    /// Sets the prompt origin position and screen size for a new line editor
    /// invocation
    ///
    /// Not to be used for resizes during a running line editor, use
    /// [`Painter::handle_resize()`] instead
    pub(crate) fn initialize_prompt_position(
        &mut self,
        suspended_state: Option<&PainterSuspendedState>,
    ) -> Result<()> {
        // Update the terminal size
        self.terminal_size = {
            let size = terminal::size()?;
            // if reported size is 0, 0 -
            // use a default size to avoid divide by 0 panics
            if size == (0, 0) {
                (80, 24)
            } else {
                size
            }
        };
        let prompt_selector = select_prompt_row(suspended_state, cursor::position()?);
        self.prompt_start_row = match prompt_selector {
            PromptRowSelector::UseExistingPrompt { start_row } => start_row,
            PromptRowSelector::MakeNewPrompt { new_row } => {
                // If we are on the last line and would move beyond the last line, we need to make
                // room for the prompt.
                // Otherwise printing the prompt would scroll off the stored prompt
                // origin, causing issues after repaints.
                if new_row == self.screen_height() {
                    self.print_crlf()?;
                    new_row.saturating_sub(1)
                } else {
                    new_row
                }
            }
        };
        Ok(())
    }

    /// Main painter for the prompt and buffer
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
        prompt_mode: PromptEditMode,
        menu: Option<&ReedlineMenu>,
        use_ansi_coloring: bool,
        cursor_config: &Option<CursorConfig>,
    ) -> Result<()> {
        self.stdout.queue(cursor::Hide)?;

        let screen_width = self.screen_width();
        let screen_height = self.screen_height();

        // Handle resize for multi line prompt
        if self.just_resized {
            self.prompt_start_row = self.prompt_start_row.saturating_sub(
                (lines.prompt_str_left.matches('\n').count()
                    + lines.prompt_indicator.matches('\n').count()) as u16,
            );
            self.just_resized = false;
        }

        // Lines and distance parameters
        let remaining_lines = self.remaining_lines();
        let required_lines = lines.required_lines(screen_width, menu);

        // Marking the painter state as larger buffer to avoid animations
        self.large_buffer = required_lines >= screen_height;

        // This might not be terribly performant. Testing it out
        let is_reset = || match cursor::position() {
            // when output something without newline, the cursor position is at current line.
            // but the prompt_start_row is next line.
            // in this case we don't want to reset, need to `add 1` to handle for such case.
            Ok(position) => position.1 + 1 < self.prompt_start_row,
            Err(_) => false,
        };

        // Moving the start position of the cursor based on the size of the required lines
        if self.large_buffer || is_reset() {
            self.prompt_start_row = 0;
        } else if required_lines >= remaining_lines {
            let extra = required_lines.saturating_sub(remaining_lines);
            self.queue_universal_scroll(extra)?;
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

        // The last_required_lines is used to calculate safe range of the current prompt.
        self.last_required_lines = required_lines;

        self.after_cursor_lines = if !lines.after_cursor.is_empty() {
            Some(lines.after_cursor.to_string())
        } else {
            None
        };

        self.stdout.queue(RestorePosition)?;

        if let Some(shapes) = cursor_config {
            let shape = match &prompt_mode {
                PromptEditMode::Emacs => shapes.emacs,
                PromptEditMode::Vi(PromptViMode::Insert) => shapes.vi_insert,
                PromptEditMode::Vi(PromptViMode::Normal) => shapes.vi_normal,
                _ => None,
            };
            if let Some(shape) = shape {
                self.stdout.queue(shape)?;
            }
        }
        self.stdout.queue(cursor::Show)?;

        self.stdout.flush()
    }

    fn print_right_prompt(&mut self, lines: &PromptLines) -> Result<()> {
        let prompt_length_right = line_width(&lines.prompt_str_right);
        let start_position = self
            .screen_width()
            .saturating_sub(prompt_length_right as u16);
        let screen_width = self.screen_width();
        let input_width = lines.estimate_right_prompt_line_width(screen_width);

        let mut row = self.prompt_start_row;
        if lines.right_prompt_on_last_line {
            row += lines.prompt_lines_with_wrap(screen_width);
        }

        if input_width <= start_position {
            self.stdout
                .queue(SavePosition)?
                .queue(cursor::MoveTo(start_position, row))?
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

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_indicator_color()))?;
        }

        self.stdout
            .queue(Print(&coerce_crlf(&lines.prompt_indicator)))?;

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_prompt_right_color()))?;
        }

        self.print_right_prompt(lines)?;

        if use_ansi_coloring {
            self.stdout
                .queue(SetAttribute(Attribute::Reset))?
                .queue(ResetColor)?;
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

        let prompt_indicator_lines = &lines.prompt_indicator.lines().count();
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
            if use_ansi_coloring {
                self.stdout
                    .queue(SetForegroundColor(prompt.get_prompt_right_color()))?;
            }

            self.print_right_prompt(lines)?;
        }

        // Adjusting extra_rows base on the calculated prompt line size
        let extra_rows = extra_rows.saturating_sub(prompt_lines);

        if use_ansi_coloring {
            self.stdout
                .queue(SetForegroundColor(prompt.get_indicator_color()))?;
        }
        let indicator_skipped = skip_buffer_lines(&lines.prompt_indicator, extra_rows, None);
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
            // This only shows the rest of the line the cursor is on
            if let Some(newline) = lines.after_cursor.find('\n') {
                self.stdout.queue(Print(&lines.after_cursor[0..newline]))?;
            } else {
                self.stdout.queue(Print(&lines.after_cursor))?;
            }
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
        self.terminal_size = (width, height);

        // `cursor::position() is blocking and can timeout.
        // The question is whether we can afford it. If not, perhaps we should use it in some scenarios but not others
        // The problem is trying to calculate this internally doesn't seem to be reliable because terminals might
        // have additional text in their buffer that messes with the offset on scroll.
        // It seems like it _should_ be ok because it only happens on resize.

        // Known bug: on iterm2 and kitty, clearing the screen via CMD-K doesn't reset
        // the position. Might need to special-case this.
        //
        // I assume this is a bug with the position() call but haven't figured that
        // out yet.
        if let Ok(position) = cursor::position() {
            self.prompt_start_row = position.1;
            self.just_resized = true;
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
        self.stdout
            .queue(Clear(ClearType::All))?
            .queue(MoveTo(0, 0))?
            .flush()?;
        self.initialize_prompt_position(None)
    }

    pub(crate) fn clear_scrollback(&mut self) -> Result<()> {
        self.stdout
            .queue(Clear(ClearType::All))?
            .queue(Clear(ClearType::Purge))?
            .queue(MoveTo(0, 0))?
            .flush()?;
        self.initialize_prompt_position(None)
    }

    // The prompt is moved to the end of the buffer after the event was handled
    pub(crate) fn move_cursor_to_end(&mut self) -> Result<()> {
        if let Some(after_cursor) = &self.after_cursor_lines {
            self.stdout
                .queue(Clear(ClearType::FromCursorDown))?
                .queue(Print(after_cursor))?;
        }
        self.print_crlf()
    }

    /// Prints an external message
    ///
    /// This function doesn't flush the buffer. So buffer should be flushed
    /// afterwards perhaps by repainting the prompt via `repaint_buffer()`.
    #[cfg(feature = "external_printer")]
    pub(crate) fn print_external_message(
        &mut self,
        messages: Vec<String>,
        line_buffer: &LineBuffer,
        prompt: &dyn Prompt,
    ) -> Result<()> {
        // adding 3 seems to be right for first line-wrap
        let prompt_len = prompt.render_prompt_right().len() + 3;
        let mut buffer_num_lines = 0_u16;
        for (i, line) in line_buffer.get_buffer().lines().enumerate() {
            let screen_lines = match i {
                0 => {
                    // the first line has to deal with the prompt
                    let first_line_len = line.len() + prompt_len;
                    // at least, it is one line
                    ((first_line_len as u16) / (self.screen_width())) + 1
                }
                _ => {
                    // the n-th line, no prompt, at least, it is one line
                    ((line.len() as u16) / self.screen_width()) + 1
                }
            };
            // count up screen-lines
            buffer_num_lines = buffer_num_lines.saturating_add(screen_lines);
        }
        // move upward to start print if the line-buffer is more than one screen-line
        if buffer_num_lines > 1 {
            self.stdout.queue(MoveUp(buffer_num_lines - 1))?;
        }
        let erase_line = format!("\r{}\r", " ".repeat(self.screen_width().into()));
        for line in messages {
            self.stdout.queue(Print(&erase_line))?;
            // Note: we don't use `print_line` here because we don't want to
            // flush right now. The subsequent repaint of the prompt will cause
            // immediate flush anyways. And if we flush here, every external
            // print causes visible flicker.
            self.stdout.queue(Print(line))?.queue(Print("\r\n"))?;
            let new_start = self.prompt_start_row.saturating_add(1);
            let height = self.screen_height();
            if new_start >= height {
                self.prompt_start_row = height - 1;
            } else {
                self.prompt_start_row = new_start;
            }
        }
        Ok(())
    }

    /// Queue scroll of `num` lines to `self.stdout`.
    ///
    /// On some platforms and terminals (e.g. windows terminal, alacritty on windows and linux)
    /// using special escape sequence '\[e<num>S' (provided by [`ScrollUp`]) does not put lines
    /// that go offscreen in scrollback history. This method prints newlines near the edge of screen
    /// (which always works) instead. See [here](https://github.com/nushell/nushell/issues/9166)
    /// for more info on subject.
    ///
    /// ## Note
    /// This method does not return cursor to the original position and leaves it at the first
    /// column of last line. **Be sure to use [`MoveTo`] afterwards if this is not the desired
    /// location**
    fn queue_universal_scroll(&mut self, num: u16) -> Result<()> {
        // If cursor is not near end of screen printing new will not scroll terminal.
        // Move it to the last line to ensure that every newline results in scroll
        self.stdout.queue(MoveTo(0, self.screen_height() - 1))?;
        for _ in 0..num {
            self.stdout.queue(Print(&coerce_crlf("\n")))?;
        }
        Ok(())
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

    #[test]
    fn test_select_new_prompt_with_no_state_no_output() {
        assert_eq!(
            select_prompt_row(None, (0, 12)),
            PromptRowSelector::MakeNewPrompt { new_row: 12 }
        );
    }

    #[test]
    fn test_select_new_prompt_with_no_state_but_output() {
        assert_eq!(
            select_prompt_row(None, (3, 12)),
            PromptRowSelector::MakeNewPrompt { new_row: 13 }
        );
    }

    #[test]
    fn test_select_existing_prompt() {
        let state = PainterSuspendedState {
            previous_prompt_rows_range: 11..=13,
        };
        assert_eq!(
            select_prompt_row(Some(&state), (0, 12)),
            PromptRowSelector::UseExistingPrompt { start_row: 11 }
        );
        assert_eq!(
            select_prompt_row(Some(&state), (3, 12)),
            PromptRowSelector::UseExistingPrompt { start_row: 11 }
        );
    }
}
