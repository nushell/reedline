use std::{io, time::Duration};

use crossterm::event;

use crate::{
    edit_mode::{EditMode, Emacs},
    enums::ReedlineEvent,
    DefaultValidator, ValidationResult, Validator,
};

use {
    crate::{
        completion::{ComplationActionHandler, DefaultCompletionActionHandler},
        core_editor::Editor,
        hinter::{DefaultHinter, Hinter},
        history::{FileBackedHistory, History, HistoryNavigationQuery},
        painter::Painter,
        prompt::{PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus},
        text_manipulation, DefaultHighlighter, EditCommand, Highlighter, Prompt, Signal,
    },
    crossterm::{cursor, event::read, terminal, Result},
    std::io::stdout,
};

#[derive(Debug, PartialEq, Eq)]
enum InputMode {
    Regular,
    HistorySearch,
    HistoryTraversal,
}

struct PromptWidget {
    offset: (u16, u16),
    origin: (u16, u16),
}

impl Default for PromptWidget {
    fn default() -> Self {
        PromptWidget {
            offset: Default::default(),
            origin: Default::default(),
        }
    }
}

impl PromptWidget {
    fn offset_columns(&self) -> u16 {
        self.offset.0
    }
    fn origin_columns(&self) -> u16 {
        self.origin.0
    }
}

/// Line editor engine
///
/// ## Example usage
/// ```no_run
/// use std::io;
/// use reedline::{Reedline, Signal, DefaultPrompt};
/// let mut line_editor = Reedline::create()?;
/// let prompt = DefaultPrompt::default();
///
/// let out = line_editor.read_line(&prompt).unwrap();
/// match out {
///    Signal::Success(content) => {
///        // process content
///    }
///    _ => {
///        eprintln!("Entry aborted!");
///
///    }
/// }
/// # Ok::<(), io::Error>(())
/// ```
pub struct Reedline {
    editor: Editor,

    // History
    history: Box<dyn History>,
    input_mode: InputMode,

    // Validator
    validator: Box<dyn Validator>,

    // Stdout
    painter: Painter,

    // Edit Mode: Vi, Emacs
    edit_mode: Box<dyn EditMode>,

    // Perform action when user hits tab
    tab_handler: Box<dyn ComplationActionHandler>,

    // Highlight the edit buffer
    highlighter: Box<dyn Highlighter>,

    // Showcase hints based on various stratiges (history, language-completion, spellcheck, etc)
    hinter: Box<dyn Hinter>,

    // UI State
    terminal_size: (u16, u16),
    prompt_widget: PromptWidget,
}

impl Reedline {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn create() -> io::Result<Reedline> {
        let history = Box::new(FileBackedHistory::default());
        let painter = Painter::new(stdout());
        let buffer_highlighter = Box::new(DefaultHighlighter::default());
        let hinter = Box::new(DefaultHinter::default());
        let validator = Box::new(DefaultValidator);

        let terminal_size = terminal::size()?;
        // Note: this is started with a garbage value
        let prompt_widget = Default::default();

        let edit_mode = Box::new(Emacs::default());

        let reedline = Reedline {
            editor: Editor::default(),
            history,
            input_mode: InputMode::Regular,
            painter,
            edit_mode,
            tab_handler: Box::new(DefaultCompletionActionHandler::default()),
            terminal_size,
            prompt_widget,
            highlighter: buffer_highlighter,
            hinter,
            validator,
        };

        Ok(reedline)
    }

    /// A builder to include the hinter in your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// //Cargo.toml
    /// //[dependencies]
    /// //nu-ansi-term = "*"
    /// use std::io;
    /// use {
    ///     nu_ansi_term::{Color, Style},
    ///     reedline::{DefaultCompleter, DefaultHinter, Reedline},
    /// };
    ///
    /// let commands = vec![
    ///     "test".into(),
    ///     "hello world".into(),
    ///     "hello world reedline".into(),
    ///     "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::create()?.with_hinter(Box::new(
    ///     DefaultHinter::default()
    ///     .with_completer(completer) // or .with_history()
    ///     // .with_inside_line()
    ///     .with_style(Style::new().italic().fg(Color::LightGray)),
    /// ));
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_hinter(mut self, hinter: Box<dyn Hinter>) -> Reedline {
        self.hinter = hinter;
        self
    }

    /// A builder to configure the completion action handler to use in your instance of the reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with tab completions support
    ///
    /// use std::io;
    /// use reedline::{DefaultCompleter, DefaultCompletionActionHandler, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::create()?.with_completion_action_handler(Box::new(
    ///   DefaultCompletionActionHandler::default().with_completer(completer),
    /// ));
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_completion_action_handler(
        mut self,
        tab_handler: Box<dyn ComplationActionHandler>,
    ) -> Reedline {
        self.tab_handler = tab_handler;
        self
    }

    /// A builder that configures the highlighter for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with highlighter support
    ///
    /// use std::io;
    /// use reedline::{DefaultHighlighter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let mut line_editor =
    /// Reedline::create()?.with_highlighter(Box::new(DefaultHighlighter::new(commands)));
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Reedline {
        self.highlighter = highlighter;
        self
    }

    /// A builder which configures the history for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with history support, including history size limits
    ///
    /// use std::io;
    /// use reedline::{FileBackedHistory, Reedline};
    ///
    /// let history = Box::new(
    /// FileBackedHistory::with_file(5, "history.txt".into())
    ///     .expect("Error configuring history with file"),
    /// );
    /// let mut line_editor = Reedline::create()?
    ///     .with_history(history)
    ///     .expect("Error configuring reedline with history");
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_history(mut self, history: Box<dyn History>) -> std::io::Result<Reedline> {
        self.history = history;

        Ok(self)
    }

    /// A builder that configures the validator for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with validator support
    ///
    /// use std::io;
    /// use reedline::{DefaultValidator, Reedline};
    ///
    /// let mut line_editor =
    /// Reedline::create()?.with_validator(Box::new(DefaultValidator));
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_validator(mut self, validator: Box<dyn Validator>) -> Reedline {
        self.validator = validator;
        self
    }

    /// A builder which configures the edit mode for your instance of the Reedline engine
    pub fn with_edit_mode(mut self, edit_mode: Box<dyn EditMode>) -> Reedline {
        self.edit_mode = edit_mode;

        self
    }

    /// Returns the corresponding expected prompt style for the given edit mode
    pub fn prompt_edit_mode(&self) -> PromptEditMode {
        self.edit_mode.edit_mode()
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
        self.painter.paint_line(msg)
    }

    /// Goes to the beginning of the next line
    ///
    /// Also works in raw mode
    pub fn print_crlf(&mut self) -> Result<()> {
        self.painter.paint_crlf()
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.painter.clear_screen()?;

        Ok(())
    }

    /// Helper implemting the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        // TODO: Should prompt be a property on the LineEditor

        // Redraw if Ctrl-L was used
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)?;
        }

        self.terminal_size = terminal::size()?;
        self.initialize_prompt(prompt)?;

        loop {
            let event = if event::poll(Duration::from_secs(1))? {
                self.get_event()?
            } else {
                ReedlineEvent::Repaint
            };

            if self.input_mode == InputMode::HistorySearch {
                if let Some(signal) = self.handle_history_search_event(prompt, event)? {
                    return Ok(signal);
                }
            } else if let Some(signal) = self.handle_event(prompt, event)? {
                return Ok(signal);
            }
        }
    }

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result bufer.
    fn history_search_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let navigation = self.history.get_navigation();

        if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
            let status = if !substring.is_empty() && self.history.string_at_cursor().is_none() {
                PromptHistorySearchStatus::Failing
            } else {
                PromptHistorySearchStatus::Passing
            };

            let prompt_history_search = PromptHistorySearch::new(status, substring);

            self.painter
                .queue_history_search_indicator(prompt, prompt_history_search)?;

            match self.history.string_at_cursor() {
                Some(string) => {
                    self.painter
                        .queue_history_search_result(&string, string.len())?;
                    self.painter.flush()?;
                }

                None => {
                    self.painter.clear_until_newline()?;
                }
            }
        }

        Ok(())
    }

    fn initialize_prompt(&mut self, prompt: &dyn Prompt) -> io::Result<()> {
        let origin = {
            let (column, row) = cursor::position()?;
            if (column, row) == (0, 0) {
                (0, 0)
            } else if row + 1 == self.terminal_rows() {
                self.painter.paint_carriage_return()?;
                (0, row.saturating_sub(1))
            } else if row + 2 == self.terminal_rows() {
                self.painter.paint_carriage_return()?;
                (0, row)
            } else {
                (0, row + 1)
            }
        };

        self.set_prompt_origin(origin);
        let prompt_offset = self.full_repaint(prompt, origin)?;
        self.set_prompt_offset(prompt_offset);

        Ok(())
    }

    fn get_event(&mut self) -> io::Result<ReedlineEvent> {
        let event = read()?;
        Ok(self.edit_mode.parse_event(event))
    }

    fn handle_history_search_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<Option<Signal>> {
        match event {
            ReedlineEvent::HandleTab => {
                let mut line_buffer = self.editor.line_buffer();
                self.tab_handler.handle(&mut line_buffer);
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.editor.reset_olds();
                    Ok(Some(Signal::CtrlD))
                } else {
                    Ok(None)
                }
            }
            ReedlineEvent::CtrlC => {
                self.editor.reset_olds();
                Ok(Some(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => {
                self.editor.reset_olds();
                Ok(Some(Signal::CtrlL))
            }
            ReedlineEvent::Enter => {
                self.queue_prompt_indicator(prompt)?;

                if let Some(string) = self.history.string_at_cursor() {
                    self.editor.set_buffer(string)
                }

                self.input_mode = InputMode::Regular;
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::Edit(commands) => {
                self.run_history_commands(&commands);
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::Mouse => Ok(None),
            ReedlineEvent::Resize(width, height) => {
                self.terminal_size = (width, height);
                // TODO properly adjusting prompt_origin on resizing while lines > 1

                let new_prompt_origin_row = cursor::position()?.1.saturating_sub(1);
                self.set_prompt_offset((
                    self.prompt_widget.origin_columns(),
                    new_prompt_origin_row,
                ));
                let new_prompt_offset = self.full_repaint(prompt, self.prompt_widget.offset)?;
                self.set_prompt_offset(new_prompt_offset);
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::Repaint => {
                if self.input_mode != InputMode::HistorySearch {
                    self.full_repaint(prompt, self.prompt_widget.origin)?;
                }
                Ok(None)
            }
            ReedlineEvent::PreviousHistory => {
                self.previous_history();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::NextHistory => {
                self.history.forward();
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history.string_at_cursor().is_none() {
                    self.history.back();
                }
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::Up => {
                self.history.back();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::Down => {
                self.history.forward();
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history.string_at_cursor().is_none() {
                    self.history.back();
                }
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::SearchHistory => {
                self.history.back();
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::None => {
                // Default no operation
                Ok(None)
            }
        }
    }

    fn handle_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<Option<Signal>> {
        match event {
            ReedlineEvent::HandleTab => {
                let mut line_buffer = self.editor.line_buffer();
                self.tab_handler.handle(&mut line_buffer);
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.editor.reset_olds();
                    Ok(Some(Signal::CtrlD))
                } else {
                    Ok(None)
                }
            }
            ReedlineEvent::CtrlC => {
                self.editor.reset_olds();
                Ok(Some(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => {
                self.editor.reset_olds();
                Ok(Some(Signal::CtrlL))
            }
            ReedlineEvent::Enter => {
                let buffer = self.editor.get_buffer().to_string();
                if matches!(self.validator.validate(&buffer), ValidationResult::Complete) {
                    self.append_to_history();
                    self.run_edit_commands(&[EditCommand::Clear], prompt)?;
                    self.print_crlf()?;
                    self.editor.reset_olds();

                    Ok(Some(Signal::Success(buffer)))
                } else {
                    self.run_edit_commands(&[EditCommand::InsertChar('\n')], prompt)?;

                    Ok(None)
                }
            }
            ReedlineEvent::Edit(commands) => {
                self.run_edit_commands(&commands, prompt)?;
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::Mouse => Ok(None),
            ReedlineEvent::Resize(width, height) => {
                self.terminal_size = (width, height);
                // TODO properly adjusting prompt_origin on resizing while lines > 1

                let new_prompt_origin_row = cursor::position()?.1.saturating_sub(1);
                self.set_prompt_offset((
                    self.prompt_widget.origin_columns(),
                    new_prompt_origin_row,
                ));
                let new_prompt_offset = self.full_repaint(prompt, self.prompt_widget.offset)?;
                self.set_prompt_offset(new_prompt_offset);
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::Repaint => {
                if self.input_mode != InputMode::HistorySearch {
                    self.full_repaint(prompt, self.prompt_widget.origin)?;
                }
                Ok(None)
            }
            ReedlineEvent::PreviousHistory => {
                self.history.back();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::NextHistory => {
                self.next_history();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::Up => {
                self.up_command();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::Down => {
                self.down_command();
                self.buffer_paint(self.prompt_widget.offset)?;
                Ok(None)
            }
            ReedlineEvent::SearchHistory => {
                self.search_history();
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::None => Ok(None),
        }
    }

    fn append_to_history(&mut self) {
        self.history.append(self.editor.get_buffer().to_string());
    }

    fn previous_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.set_history_navigation_based_on_line_buffer();
        }

        self.history.back();
        self.update_buffer_from_history();
    }

    fn next_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.set_history_navigation_based_on_line_buffer();
        }

        self.history.forward();
        self.update_buffer_from_history();
    }

    fn set_history_navigation_based_on_line_buffer(&mut self) {
        if self.editor.is_empty() || self.editor.offset() != self.editor.get_buffer().len() {
            self.history.set_navigation(HistoryNavigationQuery::Normal(
                // Hack: Hight coupling point
                self.editor.line_buffer().clone(),
            ));
        } else {
            let buffer = self.editor.get_buffer().to_string();
            self.history
                .set_navigation(HistoryNavigationQuery::PrefixSearch(buffer));
        }
    }

    fn search_history(&mut self) {
        self.input_mode = InputMode::HistorySearch;
        self.history
            .set_navigation(HistoryNavigationQuery::SubstringSearch("".to_string()));
    }

    /// Dispatches the applicable [`EditCommand`] actions for editing the history search string.
    ///
    /// Only modifies internal state, does not perform regular output!
    fn run_history_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::InsertChar(c) => {
                    let navigation = self.history.get_navigation();
                    if let HistoryNavigationQuery::SubstringSearch(mut substring) = navigation {
                        substring.push(*c);
                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(substring));
                    } else {
                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(String::from(
                                *c,
                            )))
                    }
                    self.history.back();
                }
                EditCommand::Backspace => {
                    let navigation = self.history.get_navigation();

                    if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
                        let new_substring = text_manipulation::remove_last_grapheme(&substring);

                        self.history
                            .set_navigation(HistoryNavigationQuery::SubstringSearch(
                                new_substring.to_string(),
                            ));
                        self.history.back()
                    }
                }
                _ => {
                    self.input_mode = InputMode::Regular;
                }
            }
        }
    }

    fn update_buffer_from_history(&mut self) {
        match self.history.get_navigation() {
            HistoryNavigationQuery::Normal(original) => {
                if let Some(buffer_to_paint) = self.history.string_at_cursor() {
                    self.editor.set_buffer(buffer_to_paint.clone());
                    self.set_offset(buffer_to_paint.len())
                } else {
                    // Hack
                    self.editor.set_line_buffer(original)
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                if let Some(prefix_result) = self.history.string_at_cursor() {
                    self.editor.set_buffer(prefix_result.clone());
                    self.set_offset(prefix_result.len());
                } else {
                    self.editor.set_buffer(prefix.clone());
                    self.set_offset(prefix.len());
                }
            }
            HistoryNavigationQuery::SubstringSearch(_) => todo!(),
        }
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    fn run_edit_commands(
        &mut self,
        commands: &[EditCommand],
        prompt: &dyn Prompt,
    ) -> io::Result<()> {
        if self.input_mode == InputMode::HistoryTraversal {
            if matches!(
                self.history.get_navigation(),
                HistoryNavigationQuery::Normal(_)
            ) {
                if let Some(string) = self.history.string_at_cursor() {
                    self.editor.set_buffer(string)
                }
            }
            self.input_mode = InputMode::Regular;
        }

        // Run the commands over the edit buffer
        for command in commands {
            match command {
                EditCommand::MoveToStart => self.editor.move_to_start(),
                EditCommand::MoveToEnd => self.editor.move_to_end(),
                EditCommand::MoveLeft => self.editor.move_left(),
                EditCommand::MoveRight => self.editor.move_right(),
                EditCommand::MoveWordLeft => self.editor.move_word_left(),
                EditCommand::MoveWordRight => self.editor.move_word_right(),
                // Performing mutation here might incur a perf hit down this line when
                // we would like to do multiple inserts.
                // A simple solution that we can do is to queue up these and perform the wrapping
                // check after the loop finishes. Will need to sort out the details.
                EditCommand::InsertChar(c) => {
                    let line_start = if self.editor.line() == 0 {
                        self.prompt_widget.offset_columns()
                    } else {
                        0
                    };

                    if self.might_require_wrapping(line_start, *c) {
                        let position = cursor::position()?;
                        self.editor.insert_char(*c);
                        self.wrap(position)?;
                    } else {
                        self.editor.insert_char(*c);
                    }
                    self.editor.set_previous_lines(false);

                    self.repaint(prompt)?;
                }
                EditCommand::Backspace => self.editor.backspace(),
                EditCommand::Delete => self.editor.delete(),
                EditCommand::BackspaceWord => self.editor.backspace_word(),
                EditCommand::DeleteWord => self.editor.delete_word(),
                EditCommand::Clear => self.editor.clear(),
                EditCommand::CutFromStart => self.editor.cut_from_start(),
                EditCommand::CutToEnd => self.editor.cut_from_end(),
                EditCommand::CutWordLeft => self.editor.cut_word_left(),
                EditCommand::CutWordRight => self.editor.cut_word_right(),
                EditCommand::PasteCutBuffer => self.editor.insert_cut_buffer(),
                EditCommand::UppercaseWord => self.editor.uppercase_word(),
                EditCommand::LowercaseWord => self.editor.lowercase_word(),
                EditCommand::CapitalizeChar => self.editor.capitalize_char(),
                EditCommand::SwapWords => self.editor.swap_words(),
                EditCommand::SwapGraphemes => self.editor.swap_graphemes(),
                EditCommand::Undo => self.editor.undo(),
                EditCommand::Redo => self.editor.redo(),
            }

            if [
                EditCommand::MoveToEnd,
                EditCommand::MoveToStart,
                EditCommand::MoveLeft,
                EditCommand::MoveRight,
                EditCommand::MoveWordLeft,
                EditCommand::MoveWordRight,
                EditCommand::Backspace,
                EditCommand::Delete,
                EditCommand::BackspaceWord,
                EditCommand::DeleteWord,
                EditCommand::CutFromStart,
                EditCommand::CutToEnd,
                EditCommand::CutWordLeft,
                EditCommand::CutWordRight,
            ]
            .contains(command)
            {
                self.editor.set_previous_lines(true);
            }
        }

        Ok(())
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_offset(&mut self, pos: usize) {
        self.editor.set_insertion_point(pos)
    }

    fn terminal_columns(&self) -> u16 {
        self.terminal_size.0
    }

    fn terminal_rows(&self) -> u16 {
        self.terminal_size.1
    }

    fn up_command(&mut self) {
        // If we're at the top, then:
        if !self.editor.is_cursor_at_first_line() {
            // If we're at the top, move to previous history
            self.previous_history();
        } else {
            self.editor.move_line_up();
        }
    }

    fn down_command(&mut self) {
        // If we're at the top, then:
        if !self.editor.is_cursor_at_last_line() {
            // If we're at the top, move to previous history
            self.next_history();
        } else {
            self.editor.move_line_down()
        }
    }

    fn set_prompt_offset(&mut self, offset: (u16, u16)) {
        self.prompt_widget.offset = offset;
    }

    fn set_prompt_origin(&mut self, origin: (u16, u16)) {
        self.prompt_widget.origin = origin;
    }

    fn wrap(&mut self, original_position: (u16, u16)) -> io::Result<()> {
        let (original_column, original_row) = original_position;
        self.buffer_paint(self.prompt_widget.offset)?;

        let (new_column, _) = cursor::position()?;

        if new_column < original_column && original_row + 1 == self.terminal_rows() {
            // We have wrapped off bottom of screen, and prompt is on new row
            // We need to update the prompt location in this case
            let (prompt_offset_columns, prompt_offset_rows) = self.prompt_widget.offset;
            let (prompt_origin_columns, prompt_origin_rows) = self.prompt_widget.origin;
            self.set_prompt_offset((prompt_offset_columns, prompt_offset_rows - 1));
            self.set_prompt_origin((prompt_origin_columns, prompt_origin_rows - 1));
        }

        Ok(())
    }

    fn repaint(&mut self, prompt: &dyn Prompt) -> io::Result<()> {
        // Repainting
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)?;
        } else {
            self.buffer_paint(self.prompt_widget.offset)?;
        }

        Ok(())
    }

    /// Heuristic to predetermine if we need to poll the terminal if the text wrapped around.
    fn might_require_wrapping(&self, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let terminal_width = self.terminal_columns();

        let mut test_buffer = self.editor.get_buffer().to_string();
        test_buffer.push(c);

        let display_width = UnicodeWidthStr::width(test_buffer.as_str()) + start_offset as usize;

        display_width >= terminal_width as usize
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    fn queue_prompt_indicator(&mut self, prompt: &dyn Prompt) -> Result<()> {
        // print our prompt
        let prompt_mode = self.prompt_edit_mode();
        self.painter.queue_prompt_indicator(prompt, prompt_mode)?;

        Ok(())
    }

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    fn buffer_paint(&mut self, prompt_offset: (u16, u16)) -> Result<()> {
        let cursor_position_in_buffer = self.editor.offset();
        let buffer_to_paint = self.editor.get_buffer().to_string();
        let highlighted_line = self
            .highlighter
            .highlight(&buffer_to_paint)
            .render_around_insertion_point(cursor_position_in_buffer);
        let hint: String = self.hinter.handle(
            &buffer_to_paint,
            cursor_position_in_buffer,
            self.history.as_ref(),
        );

        self.painter
            .queue_buffer(highlighted_line, hint, prompt_offset)?;
        self.painter.flush()?;

        Ok(())
    }

    fn full_repaint(
        &mut self,
        prompt: &dyn Prompt,
        prompt_origin: (u16, u16),
    ) -> Result<(u16, u16)> {
        let prompt_mode = self.prompt_edit_mode();
        let buffer_to_paint = self.editor.get_buffer().to_string();

        let cursor_position_in_buffer = self.editor.offset();
        let highlighted_line = self
            .highlighter
            .highlight(&buffer_to_paint)
            .render_around_insertion_point(cursor_position_in_buffer);
        let hint: String = self.hinter.handle(
            &buffer_to_paint,
            cursor_position_in_buffer,
            self.history.as_ref(),
        );

        self.painter.repaint_everything(
            prompt,
            prompt_mode,
            prompt_origin,
            highlighted_line,
            hint,
            self.terminal_size,
        )

        // Ok(prompt_offset)
    }
}
