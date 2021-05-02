use crate::{
    clip_buffer::{get_default_clipboard, Clipboard},
    Prompt,
};
use crate::{history::History, line_buffer::LineBuffer};
use crate::{
    history_search::{BasicSearch, BasicSearchCommand},
    line_buffer::InsertionPoint,
};
use crossterm::{
    cursor::{position, MoveTo, MoveToColumn, RestorePosition, SavePosition},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand, Result,
};
use std::{
    io::{stdout, Stdout, Write},
    time::Duration,
};

const PROMPT_COLOR: Color = Color::Blue;

pub enum EditCommand {
    MoveToStart,
    MoveToEnd,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    InsertChar(char),
    Backspace,
    Delete,
    AppendToHistory,
    PreviousHistory,
    NextHistory,
    SearchHistory,
    Clear,
    CutFromStart,
    CutToEnd,
    CutWordLeft,
    CutWordRight,
    InsertCutBuffer,
    UppercaseWord,
    LowercaseWord,
    CapitalizeChar,
    SwapWords,
    SwapGraphemes,
}

/// Line editor engine
///
/// ## Example usage
/// ```no_run
/// use reedline::{Reedline, Signal, DefaultPrompt};
/// let mut line_editor = Reedline::new();
/// let prompt = DefaultPrompt::default();
///
/// let out = line_editor.read_line(&prompt).unwrap();
/// match out {
///    Signal::Success(content) => {
///        // process content
///    }
///    _ => {
///        eprintln!("Entry aborted!");
///    }
/// }
/// ```
pub struct Reedline {
    line_buffer: LineBuffer,

    // Cut buffer
    cut_buffer: Box<dyn Clipboard>,

    // History
    history: History,
    history_search: Option<BasicSearch>, // This could be have more features in the future (fzf, configurable?)

    // Stdout
    stdout: Stdout,
}

/// Valid ways how [`Reedline::read_line()`] can return
pub enum Signal {
    /// Entry succeeded with the provided content
    Success(String),
    /// Entry was aborted with `Ctrl+C`
    CtrlC, // Interrupt current editing
    /// Abort with `Ctrl+D` signalling `EOF` or abort of a whole interactive session
    CtrlD, // End terminal session
    /// Signal to clear the current screen. Buffer content remains untouched.
    CtrlL, // FormFeed/Clear current screen
}

impl Default for Reedline {
    fn default() -> Self {
        Self::new()
    }
}

impl Reedline {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn new() -> Reedline {
        let history = History::default();
        let cut_buffer = Box::new(get_default_clipboard());
        let stdout = stdout();

        Reedline {
            line_buffer: LineBuffer::new(),
            cut_buffer,
            history,
            history_search: None,
            stdout,
        }
    }

    /// Create a new [`Reedline`] with a provided [`History`].
    /// Useful to link to a history file via [`History::with_file()`].
    pub fn with_history(history: History) -> Self {
        let mut rl = Reedline::new();
        rl.history = history;
        rl
    }

    /// Output the complete [`History`] chronologically with numbering to the terminal
    pub fn print_history(&mut self) -> Result<()> {
        let history: Vec<_> = self
            .history
            .iter_chronologic()
            .cloned()
            .enumerate()
            .collect();

        for (i, entry) in history {
            self.print_line(&format!("{}\t{}", i + 1, entry))?;
        }
        Ok(())
    }

    /// Wait for input and provide the user with a specified [`Prompt`].
    ///
    /// Returns a [`crossterm::Result`] in which the `Err` type is [`crossterm::ErrorKind`]
    /// to distinguish I/O errors and the `Ok` variant wraps a [`Signal`] which
    /// handles user inputs.
    pub fn read_line(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        terminal::enable_raw_mode()?;

        let result = self.read_line_helper(prompt);

        terminal::disable_raw_mode()?;

        result
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

    /// **For debugging purposes only:** Track the terminal events observed by [`Reedline`] and print them.
    pub fn print_events(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        let result = self.print_events_helper();
        terminal::disable_raw_mode()?;

        result
    }

    /// Dispatches the applicable [`EditCommand`] actions for editing the history search string.
    ///
    /// Only modifies internal state, does not perform regular output!
    fn run_history_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::InsertChar(c) => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::InsertChar(*c), &self.history);
                }
                EditCommand::Backspace => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::Backspace, &self.history);
                }
                EditCommand::SearchHistory => {
                    let search = self
                        .history_search
                        .as_mut()
                        .expect("couldn't get history_search as mutable"); // We checked it is some
                    search.step(BasicSearchCommand::Next, &self.history);
                }
                EditCommand::MoveRight => {
                    // Ignore move right, it is currently emited with InsertChar
                }
                // Leave history search otherwise
                _ => self.history_search = None,
            }
        }
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.history_search.is_some() {
            self.run_history_commands(commands);
            return;
        }

        for command in commands {
            match command {
                EditCommand::MoveToStart => self.line_buffer.move_to_start(),
                EditCommand::MoveToEnd => {
                    self.line_buffer.move_to_end();
                }
                EditCommand::MoveLeft => self.line_buffer.move_left(),
                EditCommand::MoveRight => self.line_buffer.move_right(),
                EditCommand::MoveWordLeft => {
                    self.line_buffer.move_word_left();
                }
                EditCommand::MoveWordRight => {
                    self.line_buffer.move_word_right();
                }
                EditCommand::InsertChar(c) => {
                    let insertion_point = self.line_buffer.insertion_point();
                    self.line_buffer.insert_char(insertion_point, *c);
                }
                EditCommand::Backspace => {
                    let left_index = self.line_buffer.grapheme_left_index();
                    let insertion_offset = self.insertion_point().offset;
                    if left_index < insertion_offset {
                        self.clear_range(left_index..insertion_offset);
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::Delete => {
                    let right_index = self.line_buffer.grapheme_right_index();
                    let insertion_offset = self.insertion_point().offset;
                    if right_index > insertion_offset {
                        self.clear_range(insertion_offset..right_index);
                    }
                }
                EditCommand::Clear => {
                    self.line_buffer.clear();
                    self.set_insertion_point(0);
                }
                EditCommand::AppendToHistory => {
                    self.history.append(self.insertion_line().to_string());
                    // reset the history cursor - we want to start at the bottom of the
                    // history again.
                    self.history.reset_cursor();
                }
                EditCommand::PreviousHistory => {
                    if let Some(history_entry) = self.history.go_back() {
                        let new_buffer = history_entry.to_string();
                        self.set_buffer(new_buffer);
                        self.move_to_end();
                    }
                }
                EditCommand::NextHistory => {
                    let new_buffer = self.history.go_forward().unwrap_or_default().to_string();
                    self.set_buffer(new_buffer);
                    self.move_to_end();
                }
                EditCommand::SearchHistory => {
                    self.history_search = Some(BasicSearch::new(self.insertion_line().to_string()));
                }
                EditCommand::CutFromStart => {
                    let insertion_offset = self.insertion_point().offset;
                    if insertion_offset > 0 {
                        self.cut_buffer
                            .set(&self.line_buffer.insertion_line()[..insertion_offset]);
                        self.clear_to_insertion_point();
                    }
                }
                EditCommand::CutToEnd => {
                    let cut_slice =
                        &self.line_buffer.insertion_line()[self.insertion_point().offset..];
                    if !cut_slice.is_empty() {
                        self.cut_buffer.set(cut_slice);
                        self.clear_to_end();
                    }
                }
                EditCommand::CutWordLeft => {
                    let insertion_offset = self.insertion_point().offset;
                    let left_index = self.line_buffer.word_left_index();
                    if left_index < insertion_offset {
                        let cut_range = left_index..insertion_offset;
                        self.cut_buffer
                            .set(&self.line_buffer.insertion_line()[cut_range.clone()]);
                        self.clear_range(cut_range);
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::CutWordRight => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let cut_range = insertion_offset..right_index;
                        self.cut_buffer
                            .set(&self.line_buffer.insertion_line()[cut_range.clone()]);
                        self.clear_range(cut_range);
                    }
                }
                EditCommand::InsertCutBuffer => {
                    let insertion_offset = self.insertion_point().offset;
                    let cut_buffer = self.cut_buffer.get();
                    self.line_buffer.insert_str(insertion_offset, &cut_buffer);
                    self.set_insertion_point(insertion_offset + cut_buffer.len());
                }
                EditCommand::UppercaseWord => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::LowercaseWord => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let lowercased = self.insertion_line()[change_range.clone()].to_lowercase();
                        self.line_buffer.replace_range(change_range, &lowercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::CapitalizeChar => {
                    if self.line_buffer.on_whitespace() {
                        self.line_buffer.move_word_right();
                        self.line_buffer.move_word_left();
                    }
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.grapheme_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::SwapWords => {
                    let old_insertion_point = self.insertion_point().offset;
                    self.line_buffer.move_word_right();
                    let word_2_end = self.insertion_point().offset;
                    self.line_buffer.move_word_left();
                    let word_2_start = self.insertion_point().offset;
                    self.line_buffer.move_word_left();
                    let word_1_start = self.insertion_point().offset;
                    let word_1_end = self.line_buffer.word_right_index();

                    if word_1_start < word_1_end
                        && word_1_end < word_2_start
                        && word_2_start < word_2_end
                    {
                        let insertion_line = self.insertion_line();
                        let word_1 = insertion_line[word_1_start..word_1_end].to_string();
                        let word_2 = insertion_line[word_2_start..word_2_end].to_string();
                        self.line_buffer
                            .replace_range(word_2_start..word_2_end, &word_1);
                        self.line_buffer
                            .replace_range(word_1_start..word_1_end, &word_2);
                        self.set_insertion_point(word_2_end);
                    } else {
                        self.set_insertion_point(old_insertion_point);
                    }
                }
                EditCommand::SwapGraphemes => {
                    let insertion_offset = self.insertion_point().offset;

                    if insertion_offset == 0 {
                        self.line_buffer.move_right()
                    } else if insertion_offset == self.line_buffer.insertion_line().len() {
                        self.line_buffer.move_left()
                    }
                    let grapheme_1_start = self.line_buffer.grapheme_left_index();
                    let grapheme_2_end = self.line_buffer.grapheme_right_index();

                    if grapheme_1_start < insertion_offset && grapheme_2_end > insertion_offset {
                        let grapheme_1 =
                            self.insertion_line()[grapheme_1_start..insertion_offset].to_string();
                        let grapheme_2 =
                            self.insertion_line()[insertion_offset..grapheme_2_end].to_string();
                        self.line_buffer
                            .replace_range(insertion_offset..grapheme_2_end, &grapheme_1);
                        self.line_buffer
                            .replace_range(grapheme_1_start..insertion_offset, &grapheme_2);
                        self.set_insertion_point(grapheme_2_end);
                    } else {
                        self.set_insertion_point(insertion_offset);
                    }
                }
            }
        }
    }

    /// Get the cursor position as understood by the underlying [`LineBuffer`]
    fn insertion_point(&self) -> InsertionPoint {
        self.line_buffer.insertion_point()
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_insertion_point(&mut self, pos: usize) {
        let mut insertion_point = self.line_buffer.insertion_point();
        insertion_point.offset = pos;

        self.line_buffer.set_insertion_point(insertion_point)
    }

    /// Get the current line of a multi-line edit [`LineBuffer`]
    fn insertion_line(&self) -> &str {
        self.line_buffer.insertion_line()
    }

    fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    fn move_to_end(&mut self) {
        self.line_buffer.move_to_end()
    }

    fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    fn maybe_wrap(&self, terminal_width: u16, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let mut test_buffer = self.insertion_line().to_string();
        test_buffer.push(c);

        let display_width = UnicodeWidthStr::width(test_buffer.as_str()) + start_offset as usize;

        display_width >= terminal_width as usize
    }

    // this fn is totally ripped off from crossterm's examples
    // it's really a diagnostic routine to see if crossterm is
    // even seeing the events. if you press a key and no events
    // are printed, it's a good chance your terminal is eating
    // those events.
    fn print_events_helper(&mut self) -> Result<()> {
        loop {
            // Wait up to 5s for another event
            if poll(Duration::from_millis(5_000))? {
                // It's guaranteed that read() wont block if `poll` returns `Ok(true)`
                let event = read()?;

                // just reuse the print_message fn to show events
                self.print_line(&format!("Event::{:?}", event))?;

                // hit the esc key to git out
                if event == Event::Key(KeyCode::Esc.into()) {
                    break;
                }
            } else {
                // Timeout expired, no event for 5s
                self.print_line("Waiting for you to type...")?;
            }
        }

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

    /// Display the complete prompt including status indicators (e.g. pwd, time)
    ///
    /// Used at the beginning of each [`Reedline::read_line()`] call.
    fn queue_prompt(&mut self, prompt: &dyn Prompt, screen_width: usize) -> Result<()> {
        // print our prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(PROMPT_COLOR))?
            .queue(Print(prompt.render_prompt(screen_width)))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    fn queue_prompt_indicator(&mut self, prompt: &dyn Prompt) -> Result<()> {
        // print our prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(PROMPT_COLOR))?
            .queue(Print(prompt.render_prompt_indicator()))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the imput buffer begins after the prompt.
    fn buffer_paint(&mut self, prompt_offset: (u16, u16)) -> Result<()> {
        let new_index = self.insertion_point().offset;

        // Repaint logic:
        //
        // Start after the prompt
        // Draw the string slice from 0 to the grapheme start left of insertion point
        // Then, get the position on the screen
        // Then draw the remainer of the buffer from above
        // Finally, reset the cursor to the saved position

        // stdout.queue(Print(&engine.line_buffer[..new_index]))?;
        let insertion_line = self.insertion_line().to_string();
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

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result bufer.
    fn history_search_paint(&mut self) -> Result<()> {
        // Assuming we are currently searching
        let search = self
            .history_search
            .as_ref()
            .expect("couldn't get history_search reference");

        let status = if search.result.is_none() && !search.search_string.is_empty() {
            "failed "
        } else {
            ""
        };

        // print search prompt
        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(Color::Blue))?
            .queue(Print(format!(
                "({}reverse-search)`{}':",
                status, search.search_string
            )))?
            .queue(ResetColor)?;

        match search.result {
            Some((history_index, offset)) => {
                let history_result = self.history.get_nth_newest(history_index).unwrap();

                self.stdout.queue(Print(&history_result[..offset]))?;
                self.stdout.queue(SavePosition)?;
                self.stdout.queue(Print(&history_result[offset..]))?;
                self.stdout.queue(Clear(ClearType::UntilNewLine))?;
                self.stdout.queue(RestorePosition)?;
            }

            None => {
                self.stdout.queue(Clear(ClearType::UntilNewLine))?;
            }
        }

        self.stdout.flush()?;

        Ok(())
    }

    /// Helper implemting the logic for [`Reeline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        terminal::enable_raw_mode()?;

        let mut terminal_size = terminal::size()?;

        self.queue_prompt(prompt, terminal_size.0 as usize)?;

        // set where the input begins
        let mut prompt_offset = position()?;

        // our line count
        let mut line_count = 1;

        // Redraw if Ctrl-L was used
        if self.history_search.is_some() {
            self.history_search_paint()?;
        } else {
            self.buffer_paint(prompt_offset)?;
        }
        self.stdout.flush()?;

        loop {
            match read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::CONTROL,
                }) => match code {
                    KeyCode::Char('d') => {
                        if self.line_buffer.is_empty() {
                            return Ok(Signal::CtrlD);
                        } else {
                            self.run_edit_commands(&[EditCommand::Delete]);
                        }
                    }
                    KeyCode::Char('a') => {
                        self.run_edit_commands(&[EditCommand::MoveToStart]);
                    }
                    KeyCode::Char('e') => {
                        self.run_edit_commands(&[EditCommand::MoveToEnd]);
                    }
                    KeyCode::Char('k') => {
                        self.run_edit_commands(&[EditCommand::CutToEnd]);
                    }
                    KeyCode::Char('u') => {
                        self.run_edit_commands(&[EditCommand::CutFromStart]);
                    }
                    KeyCode::Char('y') => {
                        self.run_edit_commands(&[EditCommand::InsertCutBuffer]);
                    }
                    KeyCode::Char('b') => {
                        self.run_edit_commands(&[EditCommand::MoveLeft]);
                    }
                    KeyCode::Char('f') => {
                        self.run_edit_commands(&[EditCommand::MoveRight]);
                    }
                    KeyCode::Char('c') => {
                        self.run_edit_commands(&[EditCommand::Clear]);
                        return Ok(Signal::CtrlC);
                    }
                    KeyCode::Char('l') => {
                        return Ok(Signal::CtrlL);
                    }
                    KeyCode::Char('h') => {
                        self.run_edit_commands(&[EditCommand::Backspace]);
                    }
                    KeyCode::Char('w') => {
                        self.run_edit_commands(&[EditCommand::CutWordLeft]);
                    }
                    KeyCode::Left => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('p') => {
                        self.run_edit_commands(&[EditCommand::PreviousHistory]);
                    }
                    KeyCode::Char('n') => {
                        self.run_edit_commands(&[EditCommand::NextHistory]);
                    }
                    KeyCode::Char('r') => {
                        self.run_edit_commands(&[EditCommand::SearchHistory]);
                    }
                    KeyCode::Char('t') => {
                        self.run_edit_commands(&[EditCommand::SwapGraphemes]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::ALT,
                }) => match code {
                    KeyCode::Char('b') => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Char('f') => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('d') => {
                        self.run_edit_commands(&[EditCommand::CutWordRight]);
                    }
                    KeyCode::Left => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('u') => {
                        self.run_edit_commands(&[EditCommand::UppercaseWord]);
                    }
                    KeyCode::Char('l') => {
                        self.run_edit_commands(&[EditCommand::LowercaseWord]);
                    }
                    KeyCode::Char('c') => {
                        self.run_edit_commands(&[EditCommand::CapitalizeChar]);
                    }
                    KeyCode::Char('t') => {
                        self.run_edit_commands(&[EditCommand::SwapWords]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent { code, modifiers: _ }) => {
                    match code {
                        KeyCode::Char(c) => {
                            let line_start = if self.insertion_point().line == 0 {
                                prompt_offset.0
                            } else {
                                0
                            };
                            if self.maybe_wrap(terminal_size.0, line_start, c) {
                                let (original_column, original_row) = position()?;
                                self.run_edit_commands(&[
                                    EditCommand::InsertChar(c),
                                    EditCommand::MoveRight,
                                ]);
                                self.buffer_paint(prompt_offset)?;

                                let (new_column, _) = position()?;

                                if new_column < original_column
                                    && original_row == (terminal_size.1 - 1)
                                    && line_count == 1
                                {
                                    // We have wrapped off bottom of screen, and prompt is on new row
                                    // We need to update the prompt location in this case
                                    prompt_offset.1 -= 1;
                                    line_count += 1;
                                }
                            } else {
                                self.run_edit_commands(&[
                                    EditCommand::InsertChar(c),
                                    EditCommand::MoveRight,
                                ]);
                            }
                        }
                        KeyCode::Backspace => {
                            self.run_edit_commands(&[EditCommand::Backspace]);
                        }
                        KeyCode::Delete => {
                            self.run_edit_commands(&[EditCommand::Delete]);
                        }
                        KeyCode::Home => {
                            self.run_edit_commands(&[EditCommand::MoveToStart]);
                        }
                        KeyCode::End => {
                            self.run_edit_commands(&[EditCommand::MoveToEnd]);
                        }
                        KeyCode::Enter => match self.history_search.clone() {
                            Some(search) => {
                                self.queue_prompt_indicator(prompt)?;
                                if let Some((history_index, _)) = search.result {
                                    self.line_buffer.set_buffer(
                                        self.history.get_nth_newest(history_index).unwrap().clone(),
                                    );
                                }
                                self.history_search = None;
                            }
                            None => {
                                let buffer = self.insertion_line().to_string();

                                self.run_edit_commands(&[
                                    EditCommand::AppendToHistory,
                                    EditCommand::Clear,
                                ]);
                                self.print_crlf()?;

                                return Ok(Signal::Success(buffer));
                            }
                        },
                        KeyCode::Up => {
                            self.run_edit_commands(&[EditCommand::PreviousHistory]);
                        }
                        KeyCode::Down => {
                            // Down means: navigate forward through the history. If we reached the
                            // bottom of the history, we clear the buffer, to make it feel like
                            // zsh/bash/whatever
                            self.run_edit_commands(&[EditCommand::NextHistory]);
                        }
                        KeyCode::Left => {
                            self.run_edit_commands(&[EditCommand::MoveLeft]);
                        }
                        KeyCode::Right => {
                            self.run_edit_commands(&[EditCommand::MoveRight]);
                        }
                        _ => {}
                    };
                }
                Event::Mouse(event) => {
                    self.print_line(&format!("{:?}", event))?;
                }
                Event::Resize(width, height) => {
                    terminal_size = (width, height);
                }
            }
            if self.history_search.is_some() {
                self.history_search_paint()?;
            } else {
                self.buffer_paint(prompt_offset)?;
            }
        }
    }
}
