use {
    crate::{
        hinter::Hinter,
        prompt::{PromptEditMode, PromptHistorySearch},
        Highlighter, History, Prompt,
    },
    crossterm::{
        cursor::{self, position, MoveTo, MoveToColumn, RestorePosition, SavePosition},
        style::{Color, Print, ResetColor, SetForegroundColor},
        terminal::{self, Clear, ClearType},
        QueueableCommand, Result,
    },
    std::io::{Stdout, Write},
};

pub struct Painter {
    // Stdout
    stdout: Stdout,

    // Buffer Highlighter
    buffer_highlighter: Box<dyn Highlighter>,

    hinter: Box<dyn Hinter>,
}

impl Painter {
    pub fn new(
        stdout: Stdout,
        buffer_highlighter: Box<dyn Highlighter>,
        hinter: Box<dyn Hinter>,
    ) -> Self {
        Painter {
            stdout,
            buffer_highlighter,
            hinter,
        }
    }

    pub fn queue_move_to(&mut self, column: u16, row: u16) -> Result<()> {
        self.stdout.queue(cursor::MoveTo(column, row))?;

        Ok(())
    }

    pub fn set_highlighter(&mut self, buffer_highlighter: Box<dyn Highlighter>) {
        self.buffer_highlighter = buffer_highlighter;
    }

    pub fn set_hinter(&mut self, hinter: Box<dyn Hinter>) {
        self.hinter = hinter;
    }

    /// Queue the complete prompt to display including status indicators (e.g. pwd, time)
    ///
    /// Used at the beginning of each [`Reedline::read_line()`] call.
    pub fn queue_prompt(
        &mut self,
        prompt: &dyn Prompt,
        prompt_mode: PromptEditMode,
        terminal_size: (u16, u16),
    ) -> Result<()> {
        let (screen_width, _) = terminal_size;

        // print our prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(prompt.get_prompt_color()))?
            .queue(Print(prompt.render_prompt(screen_width as usize)))?
            .queue(Print(prompt.render_prompt_indicator(prompt_mode)))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Queue prompt components preceding the buffer to display
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    pub fn queue_prompt_indicator(
        &mut self,
        prompt: &dyn Prompt,
        prompt_mode: PromptEditMode,
    ) -> Result<()> {
        // print our prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(prompt.get_prompt_color()))?
            .queue(Print(prompt.render_prompt_indicator(prompt_mode)))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    pub fn queue_buffer(
        &mut self,
        original_line: String,
        prompt_offset: (u16, u16),
        cursor_position_in_buffer: usize,
        history: &dyn History,
    ) -> Result<()> {
        let highlighted_line = self
            .buffer_highlighter
            .highlight(&original_line)
            .render_around_insertion_point(cursor_position_in_buffer);

        let (before_cursor, after_cursor) = highlighted_line;

        let before_cursor_lines = before_cursor.lines();
        let after_cursor_lines = after_cursor.lines();

        let mut commands = self
            .stdout
            .queue(MoveTo(prompt_offset.0, prompt_offset.1))?;

        for (idx, before_cursor_line) in before_cursor_lines.enumerate() {
            if idx != 0 {
                commands = commands.queue(Print("\r\n"))?;
            }
            commands = commands.queue(Print(before_cursor_line))?;
        }

        commands = commands
            .queue(SavePosition)?
            .queue(Print(self.hinter.handle(
                &original_line,
                cursor_position_in_buffer,
                history,
            )))?;

        for (idx, after_cursor_line) in after_cursor_lines.enumerate() {
            if idx != 0 {
                commands = commands.queue(Print("\r\n"))?;
            }
            commands = commands.queue(Print(after_cursor_line))?;
        }

        commands
            .queue(Clear(ClearType::FromCursorDown))?
            .queue(RestorePosition)?
            .flush()?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn repaint_everything(
        &mut self,
        prompt: &dyn Prompt,
        prompt_mode: PromptEditMode,
        prompt_origin: (u16, u16),
        cursor_position_in_buffer: usize,
        buffer: String,
        terminal_size: (u16, u16),
        history: &dyn History,
    ) -> Result<(u16, u16)> {
        self.stdout.queue(cursor::Hide)?;
        self.queue_move_to(prompt_origin.0, prompt_origin.1)?;
        self.queue_prompt(prompt, prompt_mode, terminal_size)?;
        self.stdout.queue(cursor::Show)?;
        self.flush()?;
        // set where the input begins
        let prompt_offset = position()?;
        self.queue_buffer(buffer, prompt_offset, cursor_position_in_buffer, history)?;
        self.stdout.queue(cursor::Show)?;
        self.flush()?;

        Ok(prompt_offset)
    }

    pub fn queue_history_search_indicator(
        &mut self,
        prompt: &dyn Prompt,
        prompt_search: PromptHistorySearch,
    ) -> Result<()> {
        // print search prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(Color::Blue))?
            .queue(Print(
                prompt.render_prompt_history_search_indicator(prompt_search),
            ))?
            .queue(ResetColor)?;

        Ok(())
    }

    pub fn queue_history_search_result(
        &mut self,
        history_result: &str,
        offset: usize,
    ) -> Result<()> {
        self.stdout
            .queue(Print(&history_result[..offset]))?
            .queue(SavePosition)?
            .queue(Print(&history_result[offset..]))?
            .queue(Clear(ClearType::UntilNewLine))?
            .queue(RestorePosition)?;

        Ok(())
    }

    /// Writes `line` to the terminal with a following carriage return and newline
    pub fn paint_line(&mut self, line: &str) -> Result<()> {
        self.stdout
            .queue(Print(line))?
            .queue(Print("\n"))?
            .queue(MoveToColumn(1))?;
        self.stdout.flush()?;

        Ok(())
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn paint_crlf(&mut self) -> Result<()> {
        self.stdout.queue(Print("\n"))?.queue(MoveToColumn(1))?;
        self.stdout.flush()?;

        Ok(())
    }

    // Printing carriage return
    pub fn paint_carriage_return(&mut self) -> Result<()> {
        self.stdout.queue(Print("\r\n\r\n"))?.flush()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        let (_, num_lines) = terminal::size()?;
        for _ in 0..2 * num_lines {
            self.stdout.queue(Print("\n"))?;
        }
        self.stdout.queue(MoveTo(0, 0))?;
        self.stdout.flush()?;

        Ok(())
    }

    pub fn clear_until_newline(&mut self) -> Result<()> {
        self.stdout.queue(Clear(ClearType::UntilNewLine))?;
        self.stdout.flush()?;

        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.stdout.flush()
    }
}
