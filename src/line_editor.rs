use crate::history::History;
use crate::{
    default_emacs_keybindings, engine::EditEngine, keybindings::Keybindings, painter::Painter,
    prompt::PromptMode, DefaultPrompt, Prompt,
};
use crate::{EditCommand, Signal};
use crossterm::{
    cursor::position,
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal, Result,
};

use std::{io::stdout, time::Duration};

trait LineEditor {
    fn print_line(&self);
    fn print_events(&self);
    fn print_crlf(&self);
    fn print_history(&self);
    fn clear_screen(&self);
    fn read_line(&self, prompt: Box<dyn Prompt>) -> Signal;
}

#[derive(Eq, PartialEq, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
}

pub struct ViLineEditor {
    painter: Painter,
    // keybindings: Keybindings,
    mode: Mode,
    partial_command: Option<char>,
    edit_engine: EditEngine,
    need_full_repaint: bool,
}

impl ViLineEditor {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn new() -> Self {
        let prompt = Box::new(DefaultPrompt::default());
        let painter = Painter::new(stdout(), prompt);

        let edit_engine = EditEngine::default();

        ViLineEditor {
            mode: Mode::Normal,
            painter,
            // keybindings: keybindings_hashmap,
            need_full_repaint: false,
            partial_command: None,
            // vi_engine: ViEngine::new(),
            edit_engine,
        }
    }

    pub fn with_history(
        mut self,
        history_file: &str,
        history_size: usize,
    ) -> std::io::Result<Self> {
        let history = History::with_file(history_size, history_file.into())?;

        // HACK: Fix this hack
        self.edit_engine.set_history(history);

        Ok(self)
    }

    pub fn prompt_mode(&self) -> PromptMode {
        match self.mode {
            Mode::Insert => PromptMode::ViInsert,
            _ => PromptMode::Normal,
        }
    }

    // painting stuff
    /// Writes `msg` to the terminal with a following carriage return and newline
    pub fn print_line(&mut self, msg: &str) -> Result<()> {
        self.painter.print_line(msg)
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn print_crlf(&mut self) -> Result<()> {
        self.painter.print_crlf()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.painter.clear_screen()
    }

    /// Output the complete [`History`] chronologically with numbering to the terminal
    pub fn print_history(&mut self) -> Result<()> {
        let history = self.edit_engine.numbered_chronological_history();

        for (i, entry) in history {
            self.painter.print_line(&format!("{}\t{}", i + 1, entry))?;
        }
        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    fn print_buffer(&mut self, prompt_offset: (u16, u16)) -> Result<()> {
        let new_index = self.edit_engine.insertion_point().offset;
        let insertion_line = self.edit_engine.insertion_line().to_string();

        self.painter
            .print_buffer(prompt_offset, new_index, insertion_line)
    }

    fn full_repaint(
        &mut self,
        prompt_origin: (u16, u16),
        terminal_width: u16,
    ) -> Result<(u16, u16)> {
        let new_index = self.edit_engine.insertion_point().offset;
        let insertion_line = self.edit_engine.insertion_line().to_string();

        self.painter.full_repaint(
            prompt_origin,
            terminal_width,
            new_index,
            insertion_line,
            self.prompt_mode(),
        )
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

    /// **For debugging purposes only:** Track the terminal events observed by [`Reedline`] and print them.
    pub fn print_events(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        let result = self.print_events_helper();
        terminal::disable_raw_mode()?;

        result
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

    fn enter_vi_insert_mode(&mut self) {
        self.mode = Mode::Insert;
        self.need_full_repaint = true;
        self.partial_command = None;
    }

    fn enter_vi_normal_mode(&mut self) {
        self.mode = Mode::Normal;
        self.need_full_repaint = true;
        self.partial_command = None;
    }

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result bufer.
    fn history_search_paint(&mut self) -> Result<()> {
        // Assuming we are currently searching
        // Hack: fixme: <Unknown>
        let search = self
            .edit_engine
            .history_search()
            .expect("couldn't get history_search reference");

        let status = if search.result.is_none() && !search.search_string.is_empty() {
            "failed "
        } else {
            ""
        };

        self.painter
            .print_search_indicator(status, &search.search_string)?;

        match search.result {
            Some((history_index, offset)) => {
                let history_result = self
                    .edit_engine
                    .history()
                    .get_nth_newest(history_index)
                    .unwrap();

                self.painter.print_history_result(history_result, offset)?;
            }

            None => {
                self.painter.clear_until_newline()?;
            }
        };

        Ok(())
    }

    fn mode(&self) -> Mode {
        self.mode
    }

    /// Heuristic to predetermine if we need to poll the terminal if the text wrapped around.
    fn maybe_wrap(&self, terminal_width: u16, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let mut test_buffer = self.edit_engine.insertion_line().to_string();
        test_buffer.push(c);

        let display_width = UnicodeWidthStr::width(test_buffer.as_str()) + start_offset as usize;

        display_width >= terminal_width as usize
    }

    fn read_line_helper(&mut self, prompt: Box<dyn Prompt>) -> Result<Signal> {
        terminal::enable_raw_mode()?;
        self.painter.set_prompt(prompt);

        let mut terminal_size = terminal::size()?;

        let prompt_origin = position()?;

        self.painter
            .print_prompt(terminal_size.0 as usize, self.prompt_mode())?;

        // set where the input begins
        let mut prompt_offset = position()?;

        // our line count
        let mut line_count = 1;

        // Redraw if Ctrl-L was used
        // HACK
        if self.edit_engine.history_search().is_some() {
            self.history_search_paint()?;
        } else {
            let new_index = self.edit_engine.insertion_point().offset;
            let insertion_line = self.edit_engine.insertion_line().to_string();
            self.painter
                .print_buffer(prompt_offset, new_index, insertion_line)?;
        }

        loop {
            match read()? {
                Event::Key(k) => match self.mode() {
                    Mode::Insert => {
                        let (code, modifier) = (k.code, k.modifiers);
                        match (modifier, code) {
                            (KeyModifiers::NONE, KeyCode::Esc) => self.enter_vi_normal_mode(),
                            (KeyModifiers::NONE, KeyCode::Char(c)) => {
                                let line_start = if self.edit_engine.insertion_point().line == 0 {
                                    prompt_offset.0
                                } else {
                                    0
                                };

                                if self.maybe_wrap(terminal_size.0, line_start, c) {
                                    let (original_column, original_row) = position()?;
                                    self.edit_engine.run_edit_commands(&[
                                        EditCommand::InsertChar(c),
                                        EditCommand::MoveRight,
                                    ]);
                                    self.print_buffer(prompt_offset)?;

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
                                    self.edit_engine.run_edit_commands(&[
                                        EditCommand::InsertChar(c),
                                        EditCommand::MoveRight,
                                    ]);
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) => {
                                match self.edit_engine.history_search().clone() {
                                    Some(search) => {
                                        self.painter.print_prompt_indicator(self.prompt_mode())?;
                                        self.edit_engine.update_buffer_with_history();
                                        self.edit_engine.clear_history_search();
                                    }
                                    None => {
                                        let buffer = self.edit_engine.insertion_line().to_string();

                                        self.edit_engine.run_edit_commands(&[
                                            EditCommand::AppendToHistory,
                                            EditCommand::Clear,
                                        ]);
                                        self.print_crlf()?;

                                        return Ok(Signal::Success(buffer));
                                    }
                                }
                            }
                            _ => {
                                panic!("not handled");
                            }
                        }
                    }
                    Mode::Normal => {
                        let (code, modifier) = (k.code, k.modifiers);
                        match (modifier, code) {
                            (KeyModifiers::NONE, KeyCode::Char('i')) => self.enter_vi_insert_mode(),
                            _ => {
                                panic!("not handled");
                            }
                        }
                    }
                },
                Event::Mouse(m) => {}
                Event::Resize(width, height) => {}
            }
            if self.edit_engine.history_search().is_some() {
                self.history_search_paint()?;
            } else if self.need_full_repaint {
                self.full_repaint(prompt_origin, terminal_size.0)?;
                self.need_full_repaint = false;
            } else {
                self.print_buffer(prompt_offset)?;
            }
        }
    }
}

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
pub struct EmacsLineEditor {
    // Stdout
    painter: Painter,

    // Keybindings
    keybindings: Keybindings,

    edit_engine: EditEngine,
}

impl Default for EmacsLineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl EmacsLineEditor {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn new() -> EmacsLineEditor {
        let prompt = Box::new(DefaultPrompt::default());
        let painter = Painter::new(stdout(), prompt);
        let edit_engine = EditEngine::default();

        EmacsLineEditor {
            painter,
            keybindings: default_emacs_keybindings(),
            edit_engine,
        }
    }

    pub fn with_history(
        mut self,
        history_file: &str,
        history_size: usize,
    ) -> std::io::Result<EmacsLineEditor> {
        let history = History::with_file(history_size, history_file.into())?;

        // HACK: Fix this hack
        self.edit_engine.set_history(history);

        Ok(self)
    }

    pub fn with_keybindings(mut self, keybindings: Keybindings) -> EmacsLineEditor {
        self.keybindings = keybindings;

        self
    }

    pub fn get_keybindings(&self) -> &Keybindings {
        &self.keybindings
    }

    pub fn update_keybindings(&mut self, keybindings: Keybindings) {
        self.keybindings = keybindings;
    }

    pub fn prompt_mode(&self) -> PromptMode {
        PromptMode::Normal
    }

    fn find_keybinding(
        &self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<Vec<EditCommand>> {
        self.keybindings.find_binding(modifier, key_code)
    }

    // painting stuff
    /// Writes `msg` to the terminal with a following carriage return and newline
    pub fn print_line(&mut self, msg: &str) -> Result<()> {
        self.painter.print_line(msg)
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn print_crlf(&mut self) -> Result<()> {
        self.painter.print_crlf()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.painter.clear_screen()
    }

    /// Output the complete [`History`] chronologically with numbering to the terminal
    pub fn print_history(&mut self) -> Result<()> {
        let history = self.edit_engine.numbered_chronological_history();

        for (i, entry) in history {
            self.painter.print_line(&format!("{}\t{}", i + 1, entry))?;
        }
        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    fn print_buffer(&mut self, prompt_offset: (u16, u16)) -> Result<()> {
        let new_index = self.edit_engine.insertion_point().offset;
        let insertion_line = self.edit_engine.insertion_line().to_string();

        self.painter
            .print_buffer(prompt_offset, new_index, insertion_line)
    }

    fn full_repaint(
        &mut self,
        prompt_origin: (u16, u16),
        terminal_width: u16,
    ) -> Result<(u16, u16)> {
        let new_index = self.edit_engine.insertion_point().offset;
        let insertion_line = self.edit_engine.insertion_line().to_string();

        self.painter.full_repaint(
            prompt_origin,
            terminal_width,
            new_index,
            insertion_line,
            self.prompt_mode(),
        )
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

    /// **For debugging purposes only:** Track the terminal events observed by [`Reedline`] and print them.
    pub fn print_events(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        let result = self.print_events_helper();
        terminal::disable_raw_mode()?;

        result
    }

    /// Heuristic to predetermine if we need to poll the terminal if the text wrapped around.
    fn maybe_wrap(&self, terminal_width: u16, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let mut test_buffer = self.edit_engine.insertion_line().to_string();
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

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result bufer.
    fn history_search_paint(&mut self) -> Result<()> {
        // Assuming we are currently searching
        // Hack: fixme: <Unknown>
        let search = self
            .edit_engine
            .history_search()
            .expect("couldn't get history_search reference");

        let status = if search.result.is_none() && !search.search_string.is_empty() {
            "failed "
        } else {
            ""
        };

        self.painter
            .print_search_indicator(status, &search.search_string)?;

        match search.result {
            Some((history_index, offset)) => {
                let history_result = self
                    .edit_engine
                    .history()
                    .get_nth_newest(history_index)
                    .unwrap();

                self.painter.print_history_result(history_result, offset)?;
            }

            None => {
                self.painter.clear_until_newline()?;
            }
        };

        Ok(())
    }

    /// Helper implemting the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: Box<dyn Prompt>) -> Result<Signal> {
        terminal::enable_raw_mode()?;
        self.painter.set_prompt(prompt);

        let mut terminal_size = terminal::size()?;

        let prompt_origin = position()?;

        self.painter
            .print_prompt(terminal_size.0 as usize, self.prompt_mode())?;

        // set where the input begins
        let mut prompt_offset = position()?;

        // our line count
        let mut line_count = 1;

        // Redraw if Ctrl-L was used
        // HACK
        if self.edit_engine.history_search().is_some() {
            self.history_search_paint()?;
        } else {
            let new_index = self.edit_engine.insertion_point().offset;
            let insertion_line = self.edit_engine.insertion_line().to_string();
            self.painter
                .print_buffer(prompt_offset, new_index, insertion_line)?;
        }

        loop {
            if poll(Duration::from_secs(1))? {
                match read()? {
                    Event::Key(KeyEvent { code, modifiers }) => {
                        match (modifiers, code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                                // TODO: <Unknown>
                                if self.edit_engine.is_line_buffer_empty() {
                                    return Ok(Signal::CtrlD);
                                } else if let Some(binding) = self.find_keybinding(modifiers, code)
                                {
                                    self.edit_engine.run_edit_commands(&binding);
                                }
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                if let Some(binding) = self.find_keybinding(modifiers, code) {
                                    self.edit_engine.run_edit_commands(&binding);
                                }
                                return Ok(Signal::CtrlC);
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                                return Ok(Signal::CtrlL);
                            }
                            (KeyModifiers::NONE, KeyCode::Char(c))
                            | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                                let line_start = if self.edit_engine.insertion_point().line == 0 {
                                    prompt_offset.0
                                } else {
                                    0
                                };
                                if self.maybe_wrap(terminal_size.0, line_start, c) {
                                    let (original_column, original_row) = position()?;
                                    self.edit_engine.run_edit_commands(&[
                                        EditCommand::InsertChar(c),
                                        EditCommand::MoveRight,
                                    ]);
                                    self.print_buffer(prompt_offset)?;

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
                                    self.edit_engine.run_edit_commands(&[
                                        EditCommand::InsertChar(c),
                                        EditCommand::MoveRight,
                                    ]);
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) => {
                                match self.edit_engine.history_search().clone() {
                                    Some(search) => {
                                        self.painter.print_prompt_indicator(self.prompt_mode())?;
                                        self.edit_engine.update_buffer_with_history();
                                        self.edit_engine.clear_history_search();
                                    }
                                    None => {
                                        let buffer = self.edit_engine.insertion_line().to_string();

                                        self.edit_engine.run_edit_commands(&[
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
                                    self.edit_engine.run_edit_commands(&binding);
                                }
                            }
                        }
                    }
                    Event::Mouse(event) => {
                        self.print_line(&format!("{:?}", event))?;
                    }
                    Event::Resize(width, height) => {
                        terminal_size = (width, height);
                        self.full_repaint(prompt_origin, width)?;
                    }
                }

                if self.edit_engine.history_search().is_some() {
                    self.history_search_paint()?;
                } else {
                    self.print_buffer(prompt_offset)?;
                }
            } else {
                self.full_repaint(prompt_origin, terminal_size.0)?;
            }
        }
    }
}
