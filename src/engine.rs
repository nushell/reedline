use std::borrow::Borrow;

use {
    crate::{
        completion::{CircularCompletionHandler, CompletionActionHandler},
        core_editor::Editor,
        edit_mode::{EditMode, Emacs},
        enums::{ReedlineEvent, UndoBehavior},
        hinter::{DefaultHinter, Hinter},
        history::{FileBackedHistory, History, HistoryNavigationQuery},
        painter::Painter,
        prompt::{PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus},
        text_manipulation, DefaultHighlighter, DefaultValidator, EditCommand, Highlighter, Prompt,
        Signal, ValidationResult, Validator,
    },
    crossterm::{cursor, event, event::Event, terminal, Result},
    std::{io, time::Duration},
    unicode_width::UnicodeWidthStr,
};

/// Determines if inputs should be used to extend the regular line buffer,
/// traverse the history in the standard prompt or edit the search string in the
/// reverse search
#[derive(Debug, PartialEq, Eq)]
enum InputMode {
    /// Regular input by user typing or previous insertion.
    /// Undo tracking is active
    Regular,
    /// Full reverse search mode with different prompt,
    /// editing affects the search string,
    /// suggestions are provided to be inserted in the line buffer
    HistorySearch,
    /// Hybrid mode indicating that history is walked through in the standard prompt
    /// Either bash style up/down history or fish style prefix search,
    /// Edits directly switch to [`InputMode::Regular`]
    HistoryTraversal,
}

#[derive(Default)]
struct PromptWidget {
    offset: (u16, u16),
    origin: (u16, u16),
}

impl PromptWidget {
    fn offset_columns(&self) -> u16 {
        self.offset.0
    }
    // fn origin_columns(&self) -> u16 {
    //     self.origin.0
    // }
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
    tab_handler: Box<dyn CompletionActionHandler>,

    // Highlight the edit buffer
    highlighter: Box<dyn Highlighter>,

    // Showcase hints based on various stratiges (history, language-completion, spellcheck, etc)
    hinter: Box<dyn Hinter>,

    // UI State
    terminal_size: (u16, u16),
    prompt_widget: PromptWidget,

    // Is Some(n) read_line() should repaint prompt every `n` milliseconds
    animate: bool,

    // Use ansi coloring or not
    use_ansi_coloring: bool,
}

impl Drop for Reedline {
    fn drop(&mut self) {
        // Ensures that the terminal is in a good state if we panic semigracefully
        // Calling `disable_raw_mode()` twice is fine with Linux
        let _ = terminal::disable_raw_mode();
    }
}

impl Reedline {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    pub fn create() -> io::Result<Reedline> {
        let history = Box::new(FileBackedHistory::default());
        let painter = Painter::new(io::stdout());
        let buffer_highlighter = Box::new(DefaultHighlighter::default());
        let hinter = Box::new(DefaultHinter::default());
        let validator = Box::new(DefaultValidator);

        let terminal_size = terminal::size()?;
        // Note: this is started with a garbage value
        let prompt_widget = PromptWidget::default();

        let edit_mode = Box::new(Emacs::default());

        let reedline = Reedline {
            editor: Editor::default(),
            history,
            input_mode: InputMode::Regular,
            painter,
            edit_mode,
            tab_handler: Box::new(CircularCompletionHandler::default()),
            terminal_size,
            prompt_widget,
            highlighter: buffer_highlighter,
            hinter,
            validator,
            animate: true,
            use_ansi_coloring: true,
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
    /// use reedline::{DefaultCompleter, CircularCompletionHandler, Reedline};
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
    ///   CircularCompletionHandler::default().with_completer(completer),
    /// ));
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn with_completion_action_handler(
        mut self,
        tab_handler: Box<dyn CompletionActionHandler>,
    ) -> Reedline {
        self.tab_handler = tab_handler;
        self
    }

    /// A builder which enables or disables the use of ansi coloring in the prompt
    /// and in the command line syntax highlighting.
    pub fn with_ansi_colors(mut self, use_ansi_coloring: bool) -> Reedline {
        self.use_ansi_coloring = use_ansi_coloring;
        self
    }

    /// A builder which enables or disables animations/automatic repainting of prompt.
    /// If `repaint` is true, every second the prompt will be repainted and the clock updates
    pub fn with_animation(mut self, repaint: bool) -> Reedline {
        self.animate = repaint;
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

        let mut crossterm_events: Vec<Event> = vec![];
        let mut reedline_events: Vec<ReedlineEvent> = vec![];

        loop {
            if event::poll(Duration::from_millis(1000))? {
                let mut latest_resize = None;

                // There could be multiple events queued up!
                // pasting text, resizes, blocking this thread (e.g. during debugging)
                // We should be able to handle all of them as quickly as possible without causing unnecessary output steps.
                while event::poll(Duration::from_millis(0))? {
                    // TODO: Maybe replace with a separate function processing the buffered event
                    match event::read()? {
                        Event::Resize(x, y) => {
                            latest_resize = Some((x, y));
                        }
                        x => crossterm_events.push(x),
                    }
                }

                if let Some((x, y)) = latest_resize {
                    reedline_events.push(ReedlineEvent::Resize(x, y));
                }

                let mut last_edit_commands = None;
                for event in crossterm_events.drain(..) {
                    match (&mut last_edit_commands, self.edit_mode.parse_event(event)) {
                        (None, ReedlineEvent::Edit(ec)) => {
                            last_edit_commands = Some(ec);
                        }
                        (None, other_event) => {
                            reedline_events.push(other_event);
                        }
                        (Some(ref mut last_ecs), ReedlineEvent::Edit(ec)) => {
                            last_ecs.extend(ec);
                        }
                        (ref mut a @ Some(_), other_event) => {
                            reedline_events.push(ReedlineEvent::Edit(a.take().unwrap()));

                            reedline_events.push(other_event);
                        }
                    }
                }
                if let Some(ec) = last_edit_commands {
                    reedline_events.push(ReedlineEvent::Edit(ec));
                }
            } else if self.animate {
                reedline_events.push(ReedlineEvent::Repaint);
            };

            for event in reedline_events.drain(..) {
                if let Some(signal) = self.handle_event(prompt, event)? {
                    return Ok(signal);
                }
            }
        }
    }

    fn handle_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> Result<Option<Signal>> {
        if self.input_mode == InputMode::HistorySearch {
            self.handle_history_search_event(prompt, event)
        } else {
            self.handle_editor_event(prompt, event)
        }
    }

    fn handle_history_search_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<Option<Signal>> {
        match event {
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.input_mode = InputMode::Regular;
                    self.editor.reset_undo_stack();
                    Ok(Some(Signal::CtrlD))
                } else {
                    self.run_history_commands(&[EditCommand::Delete]);
                    Ok(None)
                }
            }
            ReedlineEvent::CtrlC => {
                self.input_mode = InputMode::Regular;
                Ok(Some(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => Ok(Some(Signal::CtrlL)),
            ReedlineEvent::Enter | ReedlineEvent::HandleTab => {
                self.queue_prompt_indicator(prompt)?;

                if let Some(string) = self.history.string_at_cursor() {
                    self.editor.set_buffer(string);
                    self.editor.remember_undo_state(true);
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
                self.handle_resize(width, height, prompt)?;
                Ok(None)
            }
            ReedlineEvent::Repaint => {
                if self.input_mode != InputMode::HistorySearch {
                    self.full_repaint(prompt, self.prompt_widget.origin)?;
                }
                Ok(None)
            }
            ReedlineEvent::PreviousHistory | ReedlineEvent::Up | ReedlineEvent::SearchHistory => {
                self.history.back();
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::NextHistory | ReedlineEvent::Down => {
                self.history.forward();
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history.string_at_cursor().is_none() {
                    self.history.back();
                }
                self.repaint(prompt)?;
                Ok(None)
            }
            ReedlineEvent::None => {
                // Default no operation
                Ok(None)
            }
        }
    }

    /// Updates prompt origin and offset and performs a repaint to handle a screen resize event
    fn handle_resize(&mut self, width: u16, height: u16, prompt: &dyn Prompt) -> Result<()> {
        let prev_terminal_size = self.terminal_size;

        self.terminal_size = (width, height);
        // TODO properly adjusting prompt_origin on resizing while lines > 1

        let current_origin = self.prompt_widget.origin;

        if current_origin.1 >= (height - 1) {
            // Terminal is shrinking up
            // FIXME: use actual prompt size at some point
            // Note: you can't just subtract the offset from the origin,
            // as we could be shrinking so fast that the offset we read back from
            // crossterm is past where it would have been.
            self.set_prompt_origin((current_origin.0, height - 2));
        } else if prev_terminal_size.1 < height {
            // Terminal is growing down, so move the prompt down the same amount to make space
            // for history that's on the screen
            // Note: if the terminal doesn't have sufficient history, this will leave a trail
            // of previous prompts currently.
            self.set_prompt_origin((
                current_origin.0,
                current_origin.1 + (height - prev_terminal_size.1),
            ));
        }

        let prompt_offset = self.full_repaint(prompt, self.prompt_widget.origin)?;
        self.set_prompt_offset(prompt_offset);
        Ok(())
    }

    /// Repositions the prompt, if the buffer content would overflow the bottom of the screen.
    /// Checks for content that might overflow in the core buffer.
    /// Performs scrolling and updates prompt origin and offset.
    /// Does not trigger a full repaint!
    fn adjust_prompt_position(&mut self) -> Result<()> {
        let (prompt_origin_column, prompt_origin_row) = self.prompt_widget.origin;
        let (prompt_offset_column, prompt_offset_row) = self.prompt_widget.offset;

        let mut buffer_line_count = self.editor.num_lines() as u16;

        let terminal_columns = self.terminal_columns();

        // Estimate where we're going to wrap around the edge of the terminal
        for line in self.editor.line_buffer().get_buffer().lines() {
            let estimated_width = UnicodeWidthStr::width(line);

            let estimated_line_count = estimated_width as f64 / terminal_columns as f64;
            let estimated_line_count = estimated_line_count.ceil() as u64;

            // Any wrapping we estimate we might have, go ahead and add it to our line count
            buffer_line_count += (estimated_line_count - 1) as u16;
        }

        let ends_in_newline = self.editor.ends_with('\n');

        let terminal_rows = self.terminal_rows();

        if prompt_offset_row + buffer_line_count > terminal_rows {
            let spill = prompt_offset_row + buffer_line_count - terminal_rows;

            // FIXME: see if we want this as the permanent home
            if ends_in_newline {
                self.painter.scroll_rows(spill - 1)?;
            } else {
                self.painter.scroll_rows(spill)?;
            }

            // We have wrapped off bottom of screen, and prompt is on new row
            // We need to update the prompt location in this case
            self.set_prompt_offset((prompt_offset_column, prompt_offset_row - spill));
            self.set_prompt_origin((prompt_origin_column, prompt_origin_row - spill));
        }

        Ok(())
    }

    fn handle_editor_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<Option<Signal>> {
        match event {
            ReedlineEvent::HandleTab => {
                let line_buffer = self.editor.line_buffer();
                self.tab_handler.handle(line_buffer);

                let (prompt_origin_column, prompt_origin_row) = self.prompt_widget.origin;

                self.full_repaint(prompt, (prompt_origin_column, prompt_origin_row))?;
                Ok(None)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.editor.reset_undo_stack();
                    Ok(Some(Signal::CtrlD))
                } else {
                    self.run_edit_commands(&[EditCommand::Delete], prompt)?;
                    Ok(None)
                }
            }
            ReedlineEvent::CtrlC => {
                self.run_edit_commands(&[EditCommand::Clear], prompt)?;
                self.editor.reset_undo_stack();
                Ok(Some(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => Ok(Some(Signal::CtrlL)),
            ReedlineEvent::Enter => {
                let buffer = self.editor.get_buffer().to_string();
                if matches!(self.validator.validate(&buffer), ValidationResult::Complete) {
                    self.append_to_history();
                    self.run_edit_commands(&[EditCommand::Clear], prompt)?;
                    self.print_crlf()?;
                    self.editor.reset_undo_stack();

                    Ok(Some(Signal::Success(buffer)))
                } else {
                    #[cfg(windows)]
                    {
                        self.run_edit_commands(&[EditCommand::InsertChar('\r')], prompt)?;
                    }
                    self.run_edit_commands(&[EditCommand::InsertChar('\n')], prompt)?;
                    self.adjust_prompt_position()?;
                    self.full_repaint(prompt, self.prompt_widget.origin)?;

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
                self.handle_resize(width, height, prompt)?;
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

                self.adjust_prompt_position()?;
                self.full_repaint(prompt, self.prompt_widget.origin)?;
                Ok(None)
            }
            ReedlineEvent::NextHistory => {
                self.next_history();

                self.adjust_prompt_position()?;
                self.full_repaint(prompt, self.prompt_widget.origin)?;
                Ok(None)
            }
            ReedlineEvent::Up => {
                self.up_command();

                self.adjust_prompt_position()?;
                self.full_repaint(prompt, self.prompt_widget.origin)?;
                Ok(None)
            }
            ReedlineEvent::Down => {
                self.down_command();

                self.adjust_prompt_position()?;
                self.full_repaint(prompt, self.prompt_widget.origin)?;
                Ok(None)
            }
            ReedlineEvent::SearchHistory => {
                // Make sure we are able to undo the result of a reverse history search
                self.editor.remember_undo_state(true);

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

    /// Enable the search and navigation through the history from the line buffer prompt
    ///
    /// Enables either prefix search with output in the line buffer or simple traversal
    fn set_history_navigation_based_on_line_buffer(&mut self) {
        if self.editor.is_empty() || self.editor.offset() != self.editor.get_buffer().len() {
            // Perform bash-style basic up/down entry walking
            self.history.set_navigation(HistoryNavigationQuery::Normal(
                // Hack: Tight coupling point to be able to restore previously typed input
                self.editor.line_buffer().clone(),
            ));
        } else {
            // Prefix search like found in fish, zsh, etc.
            // Search string is set once from the current buffer
            // Current setup (code in other methods)
            // Continuing with typing will leave the search
            // but next invocation of this method will start the next search
            let buffer = self.editor.get_buffer().to_string();
            self.history
                .set_navigation(HistoryNavigationQuery::PrefixSearch(buffer));
        }
    }

    /// Switch into reverse history search mode
    ///
    /// This mode uses a separate prompt and handles keybindings sligthly differently!
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
                            )));
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
                        self.history.back();
                    }
                }
                _ => {
                    self.input_mode = InputMode::Regular;
                }
            }
        }
    }

    /// Set the buffer contents for history traversal/search in the standard prompt
    ///
    /// When using the up/down traversal or fish/zsh style prefix search update the main line buffer accordingly.
    /// Not used for the separate modal reverse search!
    fn update_buffer_from_history(&mut self) {
        match self.history.get_navigation() {
            HistoryNavigationQuery::Normal(original) => {
                if let Some(buffer_to_paint) = self.history.string_at_cursor() {
                    self.editor.set_buffer(buffer_to_paint.clone());
                    self.set_offset(buffer_to_paint.len());
                } else {
                    // Hack
                    self.editor.set_line_buffer(original);
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
                    self.editor.set_buffer(string);
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
                    self.editor.insert_char(*c);

                    if self.require_wrapping() {
                        let position = cursor::position()?;
                        self.wrap(position, prompt)?;
                    }

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

            match command.undo_behavior() {
                UndoBehavior::Ignore => {}
                UndoBehavior::Full => {
                    self.editor.remember_undo_state(true);
                }
                UndoBehavior::Coalesce => {
                    self.editor.remember_undo_state(false);
                }
            }
        }

        Ok(())
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_offset(&mut self, pos: usize) {
        self.editor.set_insertion_point(pos);
    }

    fn terminal_columns(&self) -> u16 {
        self.terminal_size.0
    }

    fn terminal_rows(&self) -> u16 {
        self.terminal_size.1
    }

    fn up_command(&mut self) {
        // If we're at the top, then:
        if self.editor.is_cursor_at_first_line() {
            // If we're at the top, move to previous history
            self.previous_history();
        } else {
            self.editor.move_line_up();
        }
    }

    fn down_command(&mut self) {
        // If we're at the top, then:
        if self.editor.is_cursor_at_last_line() {
            // If we're at the top, move to previous history
            self.next_history();
        } else {
            self.editor.move_line_down();
        }
    }

    fn set_prompt_offset(&mut self, offset: (u16, u16)) {
        self.prompt_widget.offset = offset;
    }

    fn set_prompt_origin(&mut self, origin: (u16, u16)) {
        self.prompt_widget.origin = origin;
    }

    /// TODO! FIX the naming and provide an accurate doccomment
    /// This function repaints and updates offsets but does not purely concern it self with wrapping
    fn wrap(&mut self, original_position: (u16, u16), prompt: &dyn Prompt) -> io::Result<()> {
        let (original_column, original_row) = original_position;
        self.buffer_paint(prompt, self.prompt_widget.offset)?;

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

    /// Heuristic to determine if we need to wrap text around.
    fn require_wrapping(&self) -> bool {
        let line_start = if self.editor.line() == 0 {
            self.prompt_widget.offset_columns()
        } else {
            0
        };

        let terminal_width = self.terminal_columns();

        let display_width = UnicodeWidthStr::width(self.editor.get_buffer()) + line_start as usize;

        display_width >= terminal_width as usize
    }

    /// Display only the prompt components preceding the buffer
    ///
    /// Used to restore the prompt indicator after a search etc. that affected
    /// the prompt
    fn queue_prompt_indicator(&mut self, prompt: &dyn Prompt) -> Result<()> {
        // print our prompt
        let prompt_mode = self.prompt_edit_mode();
        self.painter
            .queue_prompt_indicator(prompt, prompt_mode, self.use_ansi_coloring)?;

        Ok(())
    }

    /// Performs full repaint and sets the prompt origin and offset position.
    ///
    /// Prints prompt (and buffer)
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

    /// *Partial* repaint of either the buffer or the parts for reverse history search
    fn repaint(&mut self, prompt: &dyn Prompt) -> io::Result<()> {
        // Repainting
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)?;
        } else {
            self.buffer_paint(prompt, self.prompt_widget.offset)?;
        }

        Ok(())
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

    /// Repaint logic for the normal input prompt buffer
    ///
    /// Requires coordinates where the input buffer begins after the prompt.
    /// Performs highlighting and hinting at the moment!
    fn buffer_paint(&mut self, prompt: &dyn Prompt, prompt_offset: (u16, u16)) -> Result<()> {
        let cursor_position_in_buffer = self.editor.offset();
        let buffer_to_paint = self.editor.get_buffer();

        let highlighted_line = self
            .highlighter
            .highlight(buffer_to_paint)
            .render_around_insertion_point(
                cursor_position_in_buffer,
                prompt.render_prompt_multiline_indicator().borrow(),
                self.use_ansi_coloring,
            );
        let hint: String = self.hinter.handle(
            buffer_to_paint,
            cursor_position_in_buffer,
            self.history.as_ref(),
            self.use_ansi_coloring,
        );

        self.painter
            .queue_buffer(highlighted_line, hint, prompt_offset)?;
        self.painter.flush()?;

        Ok(())
    }

    /// Triggers a full repaint including the prompt parts
    ///
    /// Includes the highlighting and hinting calls.
    fn full_repaint(
        &mut self,
        prompt: &dyn Prompt,
        prompt_origin: (u16, u16),
    ) -> Result<(u16, u16)> {
        let prompt_mode = self.prompt_edit_mode();
        // let prompt_style = Style::new().fg(nu_ansi_term::Color::LightBlue);
        let buffer_to_paint = self.editor.get_buffer();
        let cursor_position_in_buffer = self.editor.offset();

        let highlighted_line = self
            .highlighter
            .highlight(buffer_to_paint)
            .render_around_insertion_point(
                cursor_position_in_buffer,
                prompt.render_prompt_multiline_indicator().borrow(),
                self.use_ansi_coloring,
            );
        let hint: String = self.hinter.handle(
            buffer_to_paint,
            cursor_position_in_buffer,
            self.history.as_ref(),
            self.use_ansi_coloring,
        );

        self.painter.repaint_everything(
            prompt,
            prompt_mode,
            prompt_origin,
            highlighted_line,
            hint,
            self.terminal_size,
            self.use_ansi_coloring,
        )
    }
}
