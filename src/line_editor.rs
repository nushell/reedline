use crate::{
    clip_buffer::{get_default_clipboard, Clipboard},
    default_emacs_keybindings,
    keybindings::Keybindings,
    painter::Painter,
    prompt::PromptMode,
    DefaultPrompt, Prompt,
};
use crate::{history::History, line_buffer::LineBuffer};
use crate::{
    history_search::{BasicSearch, BasicSearchCommand},
    line_buffer::InsertionPoint,
};
use crate::{EditCommand, Signal};
use crossterm::{
    cursor::position,
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal, Result,
};

use std::{io::stdout, time::Duration};

pub struct EditEngine {
    line_buffer: LineBuffer,
    cut_buffer: Box<dyn Clipboard>,

    // History
    history: History,
    history_search: Option<BasicSearch>, // This could be have more features in the future (fzf, configurable?)
}

impl EditEngine {
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

    /// Get the current line of a multi-line edit [`LineBuffer`]
    fn insertion_line(&self) -> &str {
        self.line_buffer.get_buffer()
    }

    fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_insertion_point(&mut self, pos: usize) {
        let mut insertion_point = self.line_buffer.insertion_point();
        insertion_point.offset = pos;

        self.line_buffer.set_insertion_point(insertion_point)
    }

    /// Get the cursor position as understood by the underlying [`LineBuffer`]
    fn insertion_point(&self) -> InsertionPoint {
        self.line_buffer.insertion_point()
    }

    fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    fn insert_char(&mut self, c: char) {
        let insertion_point = self.line_buffer.insertion_point();
        self.line_buffer.insert_char(insertion_point, c);
    }

    /// Reset the [`LineBuffer`] to be a line specified by `buffer`
    fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    fn backspace(&mut self) {
        let left_index = self.line_buffer.grapheme_left_index();
        let insertion_offset = self.insertion_point().offset;
        if left_index < insertion_offset {
            self.clear_range(left_index..insertion_offset);
            self.set_insertion_point(left_index);
        }
    }

    fn delete(&mut self) {
        let right_index = self.line_buffer.grapheme_right_index();
        let insertion_offset = self.insertion_point().offset;
        if right_index > insertion_offset {
            self.clear_range(insertion_offset..right_index);
        }
    }

    fn backspace_word(&mut self) {
        let left_word_index = self.line_buffer.word_left_index();
        self.clear_range(left_word_index..self.insertion_point().offset);
        self.set_insertion_point(left_word_index);
    }

    fn delete_word(&mut self) {
        let right_word_index = self.line_buffer.word_right_index();
        self.clear_range(self.insertion_point().offset..right_word_index);
    }

    fn clear(&mut self) {
        self.line_buffer.clear();
        self.set_insertion_point(0);
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
        let insertion_offset = self.insertion_point().offset;
        let cut_buffer = self.cut_buffer.get();
        self.line_buffer.insert_str(insertion_offset, &cut_buffer);
        self.set_insertion_point(insertion_offset + cut_buffer.len());
    }

    fn uppercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
            self.line_buffer.replace_range(change_range, &uppercased);
            self.line_buffer.move_word_right();
        }
    }

    fn lowercase_word(&mut self) {
        let insertion_offset = self.insertion_point().offset;
        let right_index = self.line_buffer.word_right_index();
        if right_index > insertion_offset {
            let change_range = insertion_offset..right_index;
            let lowercased = self.insertion_line()[change_range.clone()].to_lowercase();
            self.line_buffer.replace_range(change_range, &lowercased);
            self.line_buffer.move_word_right();
        }
    }

    fn capitalize_char(&mut self) {
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

    fn swap_words(&mut self) {
        let old_insertion_point = self.insertion_point().offset;
        self.line_buffer.move_word_right();
        let word_2_end = self.insertion_point().offset;
        self.line_buffer.move_word_left();
        let word_2_start = self.insertion_point().offset;
        self.line_buffer.move_word_left();
        let word_1_start = self.insertion_point().offset;
        let word_1_end = self.line_buffer.word_right_index();

        if word_1_start < word_1_end && word_1_end < word_2_start && word_2_start < word_2_end {
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

    fn swap_graphemes(&mut self) {
        let insertion_offset = self.insertion_point().offset;

        if insertion_offset == 0 {
            self.line_buffer.move_right()
        } else if insertion_offset == self.line_buffer.get_buffer().len() {
            self.line_buffer.move_left()
        }
        let grapheme_1_start = self.line_buffer.grapheme_left_index();
        let grapheme_2_end = self.line_buffer.grapheme_right_index();

        if grapheme_1_start < insertion_offset && grapheme_2_end > insertion_offset {
            let grapheme_1 = self.insertion_line()[grapheme_1_start..insertion_offset].to_string();
            let grapheme_2 = self.insertion_line()[insertion_offset..grapheme_2_end].to_string();
            self.line_buffer
                .replace_range(insertion_offset..grapheme_2_end, &grapheme_1);
            self.line_buffer
                .replace_range(grapheme_1_start..insertion_offset, &grapheme_2);
            self.set_insertion_point(grapheme_2_end);
        } else {
            self.set_insertion_point(insertion_offset);
        }
    }

    // History interface
    // Note: Can this be a interface rather than a concrete struct
    fn numbered_chronological_history(&self) -> Vec<(usize, String)> {
        self.history
            .iter_chronologic()
            .cloned()
            .enumerate()
            .collect()
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

    fn has_history(&self) -> bool {
        self.history_search.is_some()
    }

    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.has_history() {
            self.run_history_commands(commands);
            return;
        }

        // // Vim mode transformations
        // let commands = match self.edit_mode {
        //     EditMode::ViNormal => self.vi_engine.handle(commands),
        //     _ => commands.into(),
        // };

        // Run the commands over the edit buffer
        for command in commands {
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
                    panic!("Should not have happened");
                }
                EditCommand::EnterViNormal => {
                    panic!("Should not have happened");
                }
                _ => {}
            }

            // TODO: This seems a bit hacky, probabaly think of another approach
            // Clean-up after commands run
            for command in commands {
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
}

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
        let history = History::default();

        let prompt = Box::new(DefaultPrompt::default());
        let painter = Painter::new(stdout(), prompt);

        let edit_engine = EditEngine {
            line_buffer: LineBuffer::new(),
            cut_buffer: Box::new(get_default_clipboard()),
            history,
            history_search: None,
        };

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
        self.edit_engine.history = history;

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
            .history_search
            .as_ref()
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
                    .history
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
        if self.edit_engine.history_search.is_some() {
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
                                match self.edit_engine.history_search.clone() {
                                    Some(search) => {
                                        self.painter.print_prompt_indicator(self.prompt_mode())?;
                                        if let Some((history_index, _)) = search.result {
                                            // TODO: Unknown
                                            self.edit_engine.line_buffer.set_buffer(
                                                self.edit_engine
                                                    .history
                                                    .get_nth_newest(history_index)
                                                    .unwrap()
                                                    .clone(),
                                            );
                                        }
                                        self.edit_engine.history_search = None;
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
            if self.edit_engine.history_search.is_some() {
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
        let history = History::default();

        // keybindings_hashmap.insert(EditMode::ViInsert, default_vi_insert_keybindings());
        // keybindings_hashmap.insert(EditMode::ViNormal, default_vi_normal_keybindings());

        let prompt = Box::new(DefaultPrompt::default());
        let painter = Painter::new(stdout(), prompt);

        let edit_engine = EditEngine {
            line_buffer: LineBuffer::new(),
            cut_buffer: Box::new(get_default_clipboard()),
            history,
            history_search: None,
        };

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
        self.edit_engine.history = history;

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
            .history_search
            .as_ref()
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
                    .history
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
        if self.edit_engine.history_search.is_some() {
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
                                if self.edit_engine.line_buffer.is_empty() {
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
                                match self.edit_engine.history_search.clone() {
                                    Some(search) => {
                                        self.painter.print_prompt_indicator(self.prompt_mode())?;
                                        if let Some((history_index, _)) = search.result {
                                            // TODO: Unknown
                                            self.edit_engine.line_buffer.set_buffer(
                                                self.edit_engine
                                                    .history
                                                    .get_nth_newest(history_index)
                                                    .unwrap()
                                                    .clone(),
                                            );
                                        }
                                        self.edit_engine.history_search = None;
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

                if self.edit_engine.history_search.is_some() {
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
