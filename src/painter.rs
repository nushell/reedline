use crate::{prompt::PromptMode, Prompt};
use crossterm::{
    cursor::{position, MoveTo, MoveToColumn, RestorePosition, SavePosition},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand, Result,
};

use std::io::{Stdout, Write};

pub struct Painter {
    // Stdout
    stdout: Stdout,

    // Prompt
    prompt: Box<dyn Prompt>,
}

impl Painter {
    pub fn new(stdout: Stdout, prompt: Box<dyn Prompt>) -> Self {
        Painter { stdout, prompt }
    }

    pub fn set_prompt(&mut self, prompt: Box<dyn Prompt>) {
        self.prompt = prompt;
    }

    pub fn move_to(&mut self, column: u16, row: u16) -> Result<()> {
        self.stdout.queue(MoveTo(column, row))?;
        self.stdout.flush()?;

        Ok(())
    }

    /// Writes `msg` to the terminal with a following carriage return and newline
    pub fn print_line(&mut self, msg: &str) -> Result<()> {
        self.stdout
            .queue(Print(msg))?
            .queue(Print("\n"))?
            .queue(MoveToColumn(1))?;
        self.stdout.flush()?;

        Ok(())
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn print_crlf(&mut self) -> Result<()> {
        self.stdout.queue(Print("\n"))?.queue(MoveToColumn(1))?;
        self.stdout.flush()?;

        Ok(())
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

    // Note: Methods above this are generic, afte this point they are quite specific to this    //
    // readline

    /// Display the complete prompt including status indicators (e.g. pwd, time)
    ///
    /// Used at the beginning of each [`Reedline::read_line()`] call.
    pub fn print_prompt(&mut self, screen_width: usize, prompt_mode: PromptMode) -> Result<()> {
        // print our prompt
        // let prompt_mode = self.prompt_mode();

        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(self.prompt.get_prompt_color()))?
            .queue(Print(self.prompt.render_prompt(screen_width)))?
            .queue(Print(self.prompt.render_prompt_indicator(prompt_mode)))?
            .queue(ResetColor)?;

        self.stdout.flush()?;

        Ok(())
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    pub fn print_prompt_indicator(&mut self, prompt_mode: PromptMode) -> Result<()> {
        // print our prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(self.prompt.get_prompt_color()))?
            .queue(Print(self.prompt.render_prompt_indicator(prompt_mode)))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    pub fn print_buffer(
        &mut self,
        prompt_offset: (u16, u16),
        new_index: usize,
        insertion_line: String,
    ) -> Result<()> {
        // Repaint logic:
        //
        // Start after the prompt
        // Draw the string slice from 0 to the grapheme start left of insertion point
        // Then, get the position on the screen
        // Then draw the remainer of the buffer from above
        // Finally, reset the cursor to the saved position

        self.stdout
            .queue(MoveTo(prompt_offset.0, prompt_offset.1))?;
        self.stdout.queue(Print(&insertion_line[0..new_index]))?;
        self.stdout.queue(SavePosition)?;
        self.stdout.queue(Print(&insertion_line[new_index..]))?;
        self.stdout.queue(Clear(ClearType::FromCursorDown))?;
        self.stdout.queue(RestorePosition)?;

        self.stdout.flush()?;

        Ok(())
    }

    pub fn full_repaint(
        &mut self,
        prompt_origin: (u16, u16),
        terminal_width: u16,
        new_index: usize,
        insertion_line: String,
        prompt_mode: PromptMode,
    ) -> Result<(u16, u16)> {
        self.move_to(prompt_origin.0, prompt_origin.1)?;
        self.print_prompt(terminal_width as usize, prompt_mode)?;
        // set where the input begins
        let prompt_offset = position()?;
        self.print_buffer(prompt_offset, new_index, insertion_line)?;

        Ok(prompt_offset)
    }

    pub fn print_search_indicator(&mut self, status: &str, search_string: &str) -> Result<()> {
        // print search prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(Color::Blue))?
            .queue(Print(format!(
                "({}reverse-search)`{}':",
                status, search_string
            )))?
            .queue(ResetColor)?;

        Ok(())
    }

    pub fn print_history_result(&mut self, history_result: &String, offset: usize) -> Result<()> {
        self.stdout.queue(Print(&history_result[..offset]))?;
        self.stdout.queue(SavePosition)?;
        self.stdout.queue(Print(&history_result[offset..]))?;
        self.stdout.queue(Clear(ClearType::UntilNewLine))?;
        self.stdout.queue(RestorePosition)?;
        self.stdout.flush()?;

        Ok(())
    }
}
