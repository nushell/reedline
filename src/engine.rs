use crate::enums::EventStatus;

use {
    crate::{
        completion::{CircularCompletionHandler, CompletionActionHandler},
        context_menu::{ContextMenu, ContextMenuInput},
        core_editor::Editor,
        edit_mode::{EditMode, Emacs},
        enums::ReedlineEvent,
        highlighter::SimpleMatchHighlighter,
        hinter::{DefaultHinter, Hinter},
        history::{FileBackedHistory, History, HistoryNavigationQuery},
        painter::{Painter, PromptLines},
        prompt::{PromptEditMode, PromptHistorySearchStatus},
        text_manipulation, Completer, DefaultValidator, EditCommand, ExampleHighlighter,
        Highlighter, Prompt, PromptHistorySearch, Signal, ValidationResult, Validator,
    },
    crossterm::{event, event::Event, terminal, Result},
    std::{borrow::Borrow, io, time::Duration},
};

// These two parameters define when an event is a Paste Event. The POLL_WAIT is used
// to specify for how long the POLL should wait for events. Having a POLL_WAIT
// of zero means that every single event is treated as soon as it arrives. This
// doesn't allow for the possibility of more than 1 event happening at the same
// time.
const POLL_WAIT: u64 = 10;
// Since a paste event is multiple Event::Key events happening at the same time, we specify
// how many events should be in the crossterm_events vector before it is considered
// a paste. 10 events in 10 milliseconds is conservative enough (unlikely somebody
// will type more than 10 characters in 10 milliseconds)
const EVENTS_THRESHOLD: usize = 10;

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

    // Showcase hints based on various strategies (history, language-completion, spellcheck, etc)
    hinter: Box<dyn Hinter>,
    hide_hints: bool,

    // Is Some(n) read_line() should repaint prompt every `n` milliseconds
    animate: bool,

    // Use ansi coloring or not
    use_ansi_coloring: bool,

    // Context Menu
    context_menu: ContextMenu,
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
        let painter = Painter::new(std::io::BufWriter::new(std::io::stderr()));
        let buffer_highlighter = Box::new(ExampleHighlighter::default());
        let hinter = Box::new(DefaultHinter::default());
        let validator = Box::new(DefaultValidator);
        let context_menu = ContextMenu::default();

        let edit_mode = Box::new(Emacs::default());

        let reedline = Reedline {
            editor: Editor::default(),
            history,
            input_mode: InputMode::Regular,
            painter,
            edit_mode,
            tab_handler: Box::new(CircularCompletionHandler::default()),
            highlighter: buffer_highlighter,
            hinter,
            hide_hints: false,
            validator,
            animate: true,
            use_ansi_coloring: true,
            context_menu,
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
    ///     reedline::{DefaultHinter, Reedline},
    /// };
    ///
    /// let mut line_editor = Reedline::create()?.with_hinter(Box::new(
    ///     DefaultHinter::default()
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
    /// use reedline::{ExampleHighlighter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let mut line_editor =
    /// Reedline::create()?.with_highlighter(Box::new(ExampleHighlighter::new(commands)));
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

    /// A builder which configures the painter for debug mode
    pub fn with_debug_mode(mut self) -> Reedline {
        self.painter = Painter::new_with_debug(std::io::BufWriter::new(std::io::stderr()));

        self
    }

    /// A builder which configures the completer for the context menu
    pub fn with_menu_completer(
        mut self,
        completer: Box<dyn Completer>,
        input: ContextMenuInput,
    ) -> Reedline {
        self.context_menu = ContextMenu::new_with(completer, input);

        self
    }

    /// Returns the corresponding expected prompt style for the given edit mode
    pub fn prompt_edit_mode(&self) -> PromptEditMode {
        if self.context_menu.is_active() {
            PromptEditMode::Menu
        } else {
            self.edit_mode.edit_mode()
        }
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
    fn print_line(&mut self, msg: &str) -> Result<()> {
        self.painter.paint_line(msg)
    }

    /// Clear the screen by printing enough whitespace to start the prompt or
    /// other output back at the first line of the terminal.
    pub fn clear_screen(&mut self) -> Result<()> {
        self.painter.clear_screen()?;

        Ok(())
    }

    /// Helper implementing the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        self.painter.init_terminal_size()?;
        self.painter.initialize_prompt_position()?;
        self.hide_hints = false;

        // Redraw if Ctrl-L was used
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)?;
        } else {
            self.buffer_paint(prompt)?;
        }

        let mut crossterm_events: Vec<Event> = vec![];
        let mut reedline_events: Vec<ReedlineEvent> = vec![];

        loop {
            if event::poll(Duration::from_millis(1000))? {
                let mut latest_resize = None;

                // There could be multiple events queued up!
                // pasting text, resizes, blocking this thread (e.g. during debugging)
                // We should be able to handle all of them as quickly as possible without causing unnecessary output steps.
                while event::poll(Duration::from_millis(POLL_WAIT))? {
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
                // If the size of crossterm_event vector is larger than threshold, we could assume
                // that a lot of events were pasted into the prompt, indicating a paste
                if crossterm_events.len() > EVENTS_THRESHOLD {
                    reedline_events.push(self.handle_paste(&mut crossterm_events));
                } else {
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
                }
                if let Some(ec) = last_edit_commands {
                    reedline_events.push(ReedlineEvent::Edit(ec));
                }
            } else if self.animate && !self.painter.large_buffer {
                reedline_events.push(ReedlineEvent::Repaint);
            };

            for event in reedline_events.drain(..) {
                if let EventStatus::Exits(signal) = self.handle_event(prompt, event)? {
                    let _ = self.painter.move_cursor_to_end();
                    return Ok(signal);
                }
            }
        }
    }

    fn handle_paste(&mut self, crossterm_events: &mut Vec<Event>) -> ReedlineEvent {
        let reedline_events = crossterm_events
            .drain(..)
            .map(|event| self.edit_mode.parse_event(event))
            .collect::<Vec<ReedlineEvent>>();

        ReedlineEvent::Paste(reedline_events)
    }

    fn handle_event(&mut self, prompt: &dyn Prompt, event: ReedlineEvent) -> Result<EventStatus> {
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
    ) -> io::Result<EventStatus> {
        match event {
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.input_mode = InputMode::Regular;
                    self.editor.reset_undo_stack();
                    Ok(EventStatus::Exits(Signal::CtrlD))
                } else {
                    self.run_history_commands(&[EditCommand::Delete]);
                    Ok(EventStatus::Handled)
                }
            }
            ReedlineEvent::CtrlC => {
                self.input_mode = InputMode::Regular;
                Ok(EventStatus::Exits(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => Ok(EventStatus::Exits(Signal::CtrlL)),
            ReedlineEvent::Enter | ReedlineEvent::HistoryHintComplete => {
                if let Some(string) = self.history.string_at_cursor() {
                    self.editor.set_buffer(string);
                    self.editor.remember_undo_state(true);
                }

                self.input_mode = InputMode::Regular;
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Edit(commands) => {
                self.run_history_commands(&commands);
                self.repaint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Mouse => Ok(EventStatus::Handled),
            ReedlineEvent::Resize(width, height) => {
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                if self.input_mode != InputMode::HistorySearch {
                    self.buffer_paint(prompt)?;
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Right => Ok(EventStatus::Handled),
            ReedlineEvent::Left => Ok(EventStatus::Handled),
            ReedlineEvent::PreviousHistory | ReedlineEvent::Up | ReedlineEvent::SearchHistory => {
                self.history.back();
                self.repaint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory | ReedlineEvent::Down => {
                self.history.forward();
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history.string_at_cursor().is_none() {
                    self.history.back();
                }
                self.repaint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ContextMenu
            | ReedlineEvent::ActionHandler
            | ReedlineEvent::Paste(_)
            | ReedlineEvent::Multiple(_)
            | ReedlineEvent::UntilFound(_)
            | ReedlineEvent::None
            | ReedlineEvent::Esc
            | ReedlineEvent::MenuNext
            | ReedlineEvent::MenuPrevious
            | ReedlineEvent::MenuUp
            | ReedlineEvent::MenuDown
            | ReedlineEvent::MenuLeft
            | ReedlineEvent::MenuRight => Ok(EventStatus::Inapplicable),
        }
    }

    fn handle_editor_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<EventStatus> {
        match event {
            ReedlineEvent::ContextMenu => {
                if !self.context_menu.is_active() {
                    self.context_menu.activate();
                    self.context_menu.update_values(self.editor.line_buffer());

                    // If there is only one value in the menu, it can select be selected immediately
                    if self.context_menu.get_num_values() == 1 {
                        self.handle_editor_event(prompt, ReedlineEvent::Enter)
                    } else {
                        self.buffer_paint(prompt)?;
                        Ok(EventStatus::Handled)
                    }
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuNext => {
                if self.context_menu.is_active() {
                    self.context_menu.move_next();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuPrevious => {
                if self.context_menu.is_active() {
                    self.context_menu.move_previous();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuUp => {
                if self.context_menu.is_active() {
                    self.context_menu.move_up();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuDown => {
                if self.context_menu.is_active() {
                    self.context_menu.move_down();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuLeft => {
                if self.context_menu.is_active() {
                    self.context_menu.move_left();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::MenuRight => {
                if self.context_menu.is_active() {
                    self.context_menu.move_right();
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::HistoryHintComplete => {
                let current_hint = self.hinter.complete_hint();
                if self.hints_active()
                    && !self.context_menu.is_active()
                    && self.editor.offset() == self.editor.get_buffer().len()
                    && !current_hint.is_empty()
                {
                    self.run_edit_commands(&[EditCommand::InsertString(current_hint)]);
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::ActionHandler => {
                let line_buffer = self.editor.line_buffer();
                self.tab_handler.handle(line_buffer);
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Esc => {
                self.context_menu.deactivate();
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.editor.reset_undo_stack();
                    Ok(EventStatus::Exits(Signal::CtrlD))
                } else {
                    self.run_edit_commands(&[EditCommand::Delete]);
                    self.buffer_paint(prompt)?;
                    Ok(EventStatus::Handled)
                }
            }
            ReedlineEvent::CtrlC => {
                self.run_edit_commands(&[EditCommand::Clear]);
                self.editor.reset_undo_stack();
                Ok(EventStatus::Exits(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => Ok(EventStatus::Exits(Signal::CtrlL)),
            ReedlineEvent::Enter => {
                if self.context_menu.is_active() {
                    let line_buffer = self.editor.line_buffer();
                    let value = self.context_menu.get_value();
                    if let Some((span, value)) = value {
                        let mut offset = line_buffer.offset();
                        offset += value.len() - (span.end - span.start);

                        line_buffer.replace(span.start..span.end, &value);
                        line_buffer.set_insertion_point(offset);
                    }

                    self.context_menu.deactivate();
                    self.buffer_paint(prompt)?;

                    Ok(EventStatus::Handled)
                } else {
                    let buffer = self.editor.get_buffer().to_string();
                    if matches!(self.validator.validate(&buffer), ValidationResult::Complete) {
                        self.hide_hints = true;
                        self.buffer_paint(prompt)?;
                        self.append_to_history();
                        self.run_edit_commands(&[EditCommand::Clear]);
                        self.painter.print_crlf()?;
                        self.editor.reset_undo_stack();

                        Ok(EventStatus::Exits(Signal::Success(buffer)))
                    } else {
                        #[cfg(windows)]
                        {
                            self.run_edit_commands(&[EditCommand::InsertChar('\r')]);
                        }
                        self.run_edit_commands(&[EditCommand::InsertChar('\n')]);
                        self.buffer_paint(prompt)?;

                        Ok(EventStatus::Handled)
                    }
                }
            }
            ReedlineEvent::Edit(commands) => {
                self.run_edit_commands(&commands);
                if self.context_menu.is_active() {
                    self.context_menu.update_values(self.editor.line_buffer());
                }
                self.repaint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Mouse => Ok(EventStatus::Handled),
            ReedlineEvent::Resize(width, height) => {
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                if self.input_mode != InputMode::HistorySearch {
                    self.buffer_paint(prompt)?;
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::PreviousHistory => {
                self.previous_history();
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory => {
                self.next_history();
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Up => {
                self.up_command();
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Down => {
                self.down_command();
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Left => {
                self.run_edit_commands(&[EditCommand::MoveLeft]);
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Right => {
                self.run_edit_commands(&[EditCommand::MoveRight]);
                self.buffer_paint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::SearchHistory => {
                // Make sure we are able to undo the result of a reverse history search
                self.editor.remember_undo_state(true);

                self.enter_history_search();
                self.repaint(prompt)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Paste(events) => {
                let mut latest_signal = EventStatus::Handled;
                // Making sure that only InsertChars are handled during a paste event
                for event in events {
                    if let ReedlineEvent::Edit(commands) = event {
                        for command in commands {
                            match command {
                                EditCommand::InsertChar(c) => self.editor.insert_char(c),
                                x => {
                                    self.run_edit_commands(&[x]);
                                }
                            }
                        }
                    } else {
                        latest_signal = self.handle_editor_event(prompt, event)?;
                    }
                }

                self.buffer_paint(prompt)?;
                Ok(latest_signal)
            }
            ReedlineEvent::Multiple(events) => {
                let latest_signal = events
                    .into_iter()
                    .try_fold(EventStatus::Handled, |_, event| {
                        self.handle_editor_event(prompt, event)
                    })?;

                self.buffer_paint(prompt)?;
                Ok(latest_signal)
            }
            ReedlineEvent::UntilFound(events) => {
                for event in events {
                    match self.handle_editor_event(prompt, event)? {
                        EventStatus::Inapplicable => {
                            // Try again with the next event handler
                        }
                        success => {
                            return Ok(success);
                        }
                    }
                }
                // Exhausting the event handlers is still considered handled
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::None => Ok(EventStatus::Handled),
        }
    }

    fn append_to_history(&mut self) {
        self.history.append(self.editor.get_buffer());
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
    /// This mode uses a separate prompt and handles keybindings slightly differently!
    fn enter_history_search(&mut self) {
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
    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
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
            self.editor.run_edit_command(command);
        }
    }

    /// Set the cursor position as understood by the underlying [`LineBuffer`] for the current line
    fn set_offset(&mut self, pos: usize) {
        self.editor.set_insertion_point(pos);
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

    /// Checks if hints should be displayed and are able to be completed
    fn hints_active(&self) -> bool {
        !self.hide_hints && self.input_mode == InputMode::Regular
    }

    /// *Partial* repaint of either the buffer or the parts for reverse history search
    fn repaint(&mut self, prompt: &dyn Prompt) -> io::Result<()> {
        // Repainting
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)
        } else {
            self.buffer_paint(prompt)
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

            let prompt_history_search = PromptHistorySearch::new(status, substring.clone());

            let res_string = self
                .history
                .string_at_cursor()
                .unwrap_or_default()
                .replace("\n", "\r\n");

            // Highlight matches
            let res_string = if self.use_ansi_coloring {
                let match_highlighter = SimpleMatchHighlighter::new(substring);
                let styled = match_highlighter.highlight(&res_string);
                styled.render_simple()
            } else {
                res_string
            };

            let lines = PromptLines::new(
                prompt,
                self.prompt_edit_mode(),
                Some(prompt_history_search),
                &res_string,
                "",
                "",
            );

            self.painter
                .repaint_buffer(prompt, lines, None, self.use_ansi_coloring)?;
        }

        Ok(())
    }

    /// Triggers a full repaint including the prompt parts
    ///
    /// Includes the highlighting and hinting calls.
    fn buffer_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let cursor_position_in_buffer = self.editor.offset();
        let buffer_to_paint = self.editor.get_buffer();

        let (before_cursor, after_cursor) = self
            .highlighter
            .highlight(buffer_to_paint)
            .render_around_insertion_point(
                cursor_position_in_buffer,
                prompt.render_prompt_multiline_indicator().borrow(),
                self.use_ansi_coloring,
            );

        let hint: String = if self.hints_active() {
            self.hinter.handle(
                buffer_to_paint,
                cursor_position_in_buffer,
                self.history.as_ref(),
                self.use_ansi_coloring,
            )
        } else {
            String::new()
        };

        // Needs to add return carriage to newlines because when not in raw mode
        // some OS don't fully return the carriage
        let before_cursor = before_cursor.replace("\n", "\r\n");
        let after_cursor = after_cursor.replace("\n", "\r\n");
        let hint = hint.replace("\n", "\r\n");

        let context_menu = if self.context_menu.is_active() {
            self.context_menu
                .update_working_details(self.painter.terminal_cols());

            Some(&self.context_menu)
        } else {
            None
        };

        let lines = PromptLines::new(
            prompt,
            self.prompt_edit_mode(),
            None,
            &before_cursor,
            &after_cursor,
            &hint,
        );

        self.painter
            .repaint_buffer(prompt, lines, context_menu, self.use_ansi_coloring)
    }
}

#[test]
fn thread_safe() {
    fn f<S: Send>(_: S) {}
    f(Reedline::create().unwrap());
}
