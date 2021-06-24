use crate::{
    clip_buffer::{get_default_clipboard, Clipboard},
    default_emacs_keybindings,
    keybindings::{default_vi_insert_keybindings, default_vi_normal_keybindings, Keybindings},
    prompt::PromptMode,
    DefaultPrompt, Prompt,
};
use crate::{history::History, line_buffer::LineBuffer};
use crate::{
    history_search::{BasicSearch, BasicSearchCommand},
    line_buffer::InsertionPoint,
};
use crate::{EditCommand, EditMode, Signal, ViEngine};
use crossterm::{
    cursor,
    cursor::{position, MoveTo, MoveToColumn, RestorePosition, SavePosition},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand, Result,
};

use std::{
    collections::HashMap,
    io::{stdout, Stdout, Write},
    time::Duration,
};

/// Line editor engine
///
/// ## Example usage
/// ```no_run
/// use reedline::{Reedline, Signal, DefaultPrompt};
/// let mut line_editor = Reedline::new();
/// let prompt = Box::new(DefaultPrompt::default());
///
/// let out = line_editor.read_line(prompt).unwrap();
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

    // Keybindings
    keybindings: HashMap<EditMode, Keybindings>,

    // Edit mode
    edit_mode: EditMode,

    // Prompt
    prompt: Box<dyn Prompt>,

    // Dirty bits
    need_full_repaint: bool,

    // Partial command
    partial_command: Option<char>,

    // Vi normal mode state engine
    vi_engine: ViEngine,
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
        let mut keybindings_hashmap = HashMap::new();
        keybindings_hashmap.insert(EditMode::Emacs, default_emacs_keybindings());
        keybindings_hashmap.insert(EditMode::ViInsert, default_vi_insert_keybindings());
        keybindings_hashmap.insert(EditMode::ViNormal, default_vi_normal_keybindings());

        Reedline {
            line_buffer: LineBuffer::new(),
            cut_buffer,
            history,
            history_search: None,
            stdout,
            keybindings: keybindings_hashmap,
            edit_mode: EditMode::Emacs,
            prompt: Box::new(DefaultPrompt::default()),
            need_full_repaint: false,
            partial_command: None,
            vi_engine: ViEngine::new(),
        }
    }

    pub fn with_history(
        mut self,
        history_file: &str,
        history_size: usize,
    ) -> std::io::Result<Reedline> {
        let history = History::with_file(history_size, history_file.into())?;

        self.history = history;

        Ok(self)
    }

    pub fn with_keybindings(mut self, keybindings: Keybindings) -> Reedline {
        self.keybindings.insert(EditMode::Emacs, keybindings);

        self
    }

    pub fn with_edit_mode(mut self, edit_mode: EditMode) -> Reedline {
        self.edit_mode = edit_mode;

        self
    }

    pub fn get_keybindings(&self) -> &Keybindings {
        &self
            .keybindings
            .get(&EditMode::Emacs)
            .expect("Internal error: emacs should always be supported")
    }

    pub fn update_keybindings(&mut self, keybindings: Keybindings) {
        self.keybindings.insert(EditMode::Emacs, keybindings);
    }

    pub fn edit_mode(&self) -> EditMode {
        self.edit_mode
    }

    pub fn prompt_mode(&self) -> PromptMode {
        match self.edit_mode {
            EditMode::ViInsert => PromptMode::ViInsert,
            _ => PromptMode::Normal,
        }
    }

    fn find_keybinding(
        &self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<Vec<EditCommand>> {
        self.keybindings
            .get(&self.edit_mode)
            .expect("Internal error: expected to find keybindings for edit mode")
            .find_binding(modifier, key_code)
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

    pub fn move_to(&mut self, column: u16, row: u16) -> Result<()> {
        self.stdout.queue(MoveTo(column, row))?;
        self.stdout.flush()?;

        Ok(())
    }

    /// Wait for input and provide the user with a specified [`Prompt`].
    ///
    /// Returns a [`crossterm::Result`] in which the `Err` type is [`crossterm::ErrorKind`]
    /// to distinguish I/O errors and the `Ok` variant wraps a [`Signal`] which
    /// handles user inputs.
    pub fn read_line(&mut self, prompt: Box<dyn Prompt>) -> Result<Signal> {
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

    fn move_to_start(&mut self) {
        self.line_buffer.move_to_start()
    }

    fn move_to_end(&mut self) {
        self.line_buffer.move_to_end()
    }

    fn move_left(&mut self) {
        self.line_buffer.move_left()
    }

    fn move_right(&mut self) {
        self.line_buffer.move_right()
    }

    fn move_word_left(&mut self) {
        self.line_buffer.move_word_left();
    }

    fn move_word_right(&mut self) {
        self.line_buffer.move_right()
    }

    fn insert_char(&mut self, c: char) {
        self.line_buffer.insert_char(c)
    }

    fn backspace(&mut self) {
        self.line_buffer.delete_left_grapheme();
    }

    fn delete(&mut self) {
        self.line_buffer.delete_right_grapheme();
    }

    fn backspace_word(&mut self) {
        self.line_buffer.delete_word_left();
    }

    fn delete_word(&mut self) {
        self.line_buffer.delete_word_right();
    }

    fn clear(&mut self) {
        self.line_buffer.clear();
    }

    fn append_to_history(&mut self) {
        self.history.append(self.insertion_line().to_string());
    }

    fn previous_history(&mut self) {
        if self.history.history_prefix.is_none() {
            let buffer = self.line_buffer.get_buffer();
            self.history.history_prefix = Some(buffer.to_owned());
        }

        if let Some(history_entry) = self.history.go_back_with_prefix() {
            let new_buffer = history_entry.to_string();
            self.set_buffer(new_buffer);
            self.move_to_end();
        }
    }

    fn next_history(&mut self) {
        if self.history.history_prefix.is_none() {
            let buffer = self.line_buffer.get_buffer();
            self.history.history_prefix = Some(buffer.to_owned());
        }

        if let Some(history_entry) = self.history.go_forward_with_prefix() {
            let new_buffer = history_entry.to_string();
            self.set_buffer(new_buffer);
            self.move_to_end();
        }
    }

    fn search_history(&mut self) {
        self.history_search = Some(BasicSearch::new(self.insertion_line().to_string()));
    }

    fn cut_from_start(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        if insertion_offset > 0 {
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[..insertion_offset]);
            self.clear_to_insertion_point();
        }
    }

    fn cut_from_end(&mut self) {
        let cut_slice = &self.line_buffer.get_buffer()[self.insertion_point().offset..];
        if !cut_slice.is_empty() {
            self.cut_buffer.set(cut_slice);
            self.clear_to_end();
        }
    }

    fn cut_word_left(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let left_index = self.line_buffer.word_left_index();
        if left_index < insertion_offset {
            let cut_range = left_index..insertion_offset;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
            self.set_insertion_point(left_index);
        }
    }

    fn cut_word_right(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let cut_range = insertion_offset..right_index;
            self.cut_buffer
                .set(&self.line_buffer.get_buffer()[cut_range.clone()]);
            self.clear_range(cut_range);
        }
    }

    fn insert_cut_buffer(&mut self) {
        let cut_buffer = self.cut_buffer.get();
        self.line_buffer.insert_str(&cut_buffer);
    }

    fn uppercase_word(&mut self) {
        self.line_buffer.uppercase_word();
    }

    fn lowercase_word(&mut self) {
        self.line_buffer.lowercase_word();
    }

    fn capitalize_char(&mut self) {
        self.line_buffer.capitalize_char();
    }

    fn swap_words(&mut self) {
        self.line_buffer.swap_words();
    }

    fn swap_graphemes(&mut self) {
        self.line_buffer.swap_graphemes();
    }

    fn enter_vi_insert_mode(&mut self) {
        self.edit_mode = EditMode::ViInsert;
        self.need_full_repaint = true;
        self.partial_command = None;
    }

    fn enter_vi_normal_mode(&mut self) {
        self.edit_mode = EditMode::ViNormal;
        self.need_full_repaint = true;
        self.partial_command = None;
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.history_search.is_some() {
            self.run_history_commands(commands);
            return;
        }

        // Vim mode transformations
        let commands = match self.edit_mode {
            EditMode::ViNormal => self.vi_engine.handle(commands),
            _ => commands.into(),
        };

        // Run the commands over the edit buffer
        for command in &commands {
            match command {
                EditCommand::MoveToStart => self.move_to_start(),
                EditCommand::MoveToEnd => {
                    self.move_to_end();
                }
                EditCommand::MoveLeft => self.move_left(),
                EditCommand::MoveRight => self.move_right(),
                EditCommand::MoveWordLeft => {
                    self.move_word_left();
                }
                EditCommand::MoveWordRight => {
                    self.move_word_right();
                }
                EditCommand::InsertChar(c) => {
                    self.insert_char(*c);
                }
                EditCommand::Backspace => {
                    self.backspace();
                }
                EditCommand::Delete => {
                    self.delete();
                }
                EditCommand::BackspaceWord => {
                    self.backspace_word();
                }
                EditCommand::DeleteWord => {
                    self.delete_word();
                }
                EditCommand::Clear => {
                    self.clear();
                }
                EditCommand::AppendToHistory => {
                    self.append_to_history();
                }
                EditCommand::PreviousHistory => {
                    self.previous_history();
                }
                EditCommand::NextHistory => {
                    self.next_history();
                }
                EditCommand::SearchHistory => {
                    self.search_history();
                }
                EditCommand::CutFromStart => {
                    self.cut_from_start();
                }
                EditCommand::CutToEnd => {
                    self.cut_from_end();
                }
                EditCommand::CutWordLeft => {
                    self.cut_word_left();
                }
                EditCommand::CutWordRight => {
                    self.cut_word_right();
                }
                EditCommand::PasteCutBuffer => {
                    self.insert_cut_buffer();
                }
                EditCommand::UppercaseWord => {
                    self.uppercase_word();
                }
                EditCommand::LowercaseWord => {
                    self.lowercase_word();
                }
                EditCommand::CapitalizeChar => {
                    self.capitalize_char();
                }
                EditCommand::SwapWords => {
                    self.swap_words();
                }
                EditCommand::SwapGraphemes => {
                    self.swap_graphemes();
                }
                EditCommand::EnterViInsert => {
                    self.enter_vi_insert_mode();
                }
                EditCommand::EnterViNormal => {
                    self.enter_vi_normal_mode();
                }
                _ => {}
            }

            // Clean-up after commands run
            for command in &commands {
                match command {
                    EditCommand::PreviousHistory => {}
                    EditCommand::NextHistory => {}
                    _ => {
                        // Clean up the old prefix used for history search
                        if self.history.history_prefix.is_some() {
                            self.history.history_prefix = None;
                        }
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
        self.line_buffer.get_buffer()
    }

    /// Reset the [`LineBuffer`] to be a line specified by `buffer`
    fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
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

    /// Heuristic to predetermine if we need to poll the terminal if the text wrapped around.
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
    fn queue_prompt(&mut self, screen_width: usize) -> Result<()> {
        // print our prompt
        let prompt_mode = self.prompt_mode();

        self.stdout
            .queue(MoveToColumn(0))?
            .queue(SetForegroundColor(self.prompt.get_prompt_color()))?
            .queue(Print(self.prompt.render_prompt(screen_width)))?
            .queue(Print(self.prompt.render_prompt_indicator(prompt_mode)))?
            .queue(ResetColor)?;

        Ok(())
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    fn queue_prompt_indicator(&mut self) -> Result<()> {
        // print our prompt
        let prompt_mode = self.prompt_mode();
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

    fn full_repaint(
        &mut self,
        prompt_origin: (u16, u16),
        terminal_width: u16,
    ) -> Result<(u16, u16)> {
        self.stdout
            .queue(cursor::Hide)?
            .queue(MoveTo(prompt_origin.0, prompt_origin.1))?;
        self.queue_prompt(terminal_width as usize)?;
        // set where the input begins
        self.stdout.queue(cursor::Show)?.flush()?;

        let prompt_offset = position()?;
        self.buffer_paint(prompt_offset)?;
        self.stdout.queue(cursor::Show)?.flush()?;

        Ok(prompt_offset)
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

    /// Helper implemting the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: Box<dyn Prompt>) -> Result<Signal> {
        terminal::enable_raw_mode()?;
        self.prompt = prompt;

        let mut terminal_size = terminal::size()?;

        let mut prompt_origin = {
            let (column, row) = position()?;
            if (column, row) == (0, 0) {
                (0, 0)
            } else if row + 1 == terminal_size.1 {
                self.stdout.queue(Print("\r\n\r\n"))?.flush()?;
                (0, row.saturating_sub(1))
            } else if row + 2 == terminal_size.1 {
                self.stdout.queue(Print("\r\n\r\n"))?.flush()?;
                (0, row)
            } else {
                (0, row + 1)
            }
        };

        // set where the input begins
        let mut prompt_offset = self.full_repaint(prompt_origin, terminal_size.0)?;

        // Redraw if Ctrl-L was used
        if self.history_search.is_some() {
            self.history_search_paint()?;
        }

        loop {
            if poll(Duration::from_secs(1))? {
                match read()? {
                    Event::Key(KeyEvent { code, modifiers }) => {
                        match (modifiers, code, self.edit_mode) {
                            (KeyModifiers::CONTROL, KeyCode::Char('d'), _) => {
                                if self.line_buffer.is_empty() {
                                    return Ok(Signal::CtrlD);
                                } else if let Some(binding) = self.find_keybinding(modifiers, code)
                                {
                                    self.run_edit_commands(&binding);
                                }
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('c'), _) => {
                                if let Some(binding) = self.find_keybinding(modifiers, code) {
                                    self.run_edit_commands(&binding);
                                }
                                return Ok(Signal::CtrlC);
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('l'), EditMode::Emacs) => {
                                return Ok(Signal::CtrlL);
                            }
                            (KeyModifiers::NONE, KeyCode::Char(c), x)
                            | (KeyModifiers::SHIFT, KeyCode::Char(c), x)
                                if x == EditMode::ViNormal =>
                            {
                                self.run_edit_commands(&[EditCommand::ViCommandFragment(c)]);
                            }
                            (KeyModifiers::NONE, KeyCode::Char(c), x)
                            | (KeyModifiers::SHIFT, KeyCode::Char(c), x)
                                if x != EditMode::ViNormal =>
                            {
                                let line_start = if self.insertion_point().line == 0 {
                                    prompt_offset.0
                                } else {
                                    0
                                };
                                if self.maybe_wrap(terminal_size.0, line_start, c) {
                                    let (original_column, original_row) = position()?;
                                    self.run_edit_commands(&[EditCommand::InsertChar(c)]);
                                    self.buffer_paint(prompt_offset)?;

                                    let (new_column, _) = position()?;

                                    if new_column < original_column
                                        && original_row + 1 == (terminal_size.1)
                                    {
                                        // We have wrapped off bottom of screen, and prompt is on new row
                                        // We need to update the prompt location in this case
                                        prompt_origin.1 -= 1;
                                        prompt_offset.1 -= 1;
                                    }
                                } else {
                                    self.run_edit_commands(&[EditCommand::InsertChar(c)]);
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Enter, x) if x != EditMode::ViNormal => {
                                match self.history_search.clone() {
                                    Some(search) => {
                                        self.queue_prompt_indicator()?;
                                        if let Some((history_index, _)) = search.result {
                                            self.line_buffer.set_buffer(
                                                self.history
                                                    .get_nth_newest(history_index)
                                                    .unwrap()
                                                    .clone(),
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
                                }
                            }

                            _ => {
                                if let Some(binding) = self.find_keybinding(modifiers, code) {
                                    self.run_edit_commands(&binding);
                                }
                            }
                        }
                    }
                    Event::Mouse(event) => {
                        self.print_line(&format!("{:?}", event))?;
                    }
                    Event::Resize(width, height) => {
                        terminal_size = (width, height);
                        // TODO properly adjusting prompt_origin on resizing while lines > 1
                        prompt_origin.1 = position()?.1.saturating_sub(1);
                        prompt_offset = self.full_repaint(prompt_origin, width)?;
                        continue;
                    }
                }
                if self.history_search.is_some() {
                    self.history_search_paint()?;
                } else if self.need_full_repaint {
                    prompt_offset = self.full_repaint(prompt_origin, terminal_size.0)?;
                    self.need_full_repaint = false;
                } else {
                    self.buffer_paint(prompt_offset)?;
                }
            } else {
                prompt_offset = self.full_repaint(prompt_origin, terminal_size.0)?;
            }
        }
    }
}
