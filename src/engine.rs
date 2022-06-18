#[cfg(feature = "bashisms")]
use crate::{
    history::SearchFilter,
    menu_functions::{parse_selection_char, ParseAction},
};

use crate::result::{ReedlineError, ReedlineErrorVariants};
use {
    crate::{
        completion::{CircularCompletionHandler, Completer, DefaultCompleter},
        core_editor::Editor,
        edit_mode::{EditMode, Emacs},
        enums::{EventStatus, ReedlineEvent},
        highlighter::SimpleMatchHighlighter,
        hinter::Hinter,
        history::{
            FileBackedHistory, History, HistoryCursor, HistoryItem, HistoryItemId,
            HistoryNavigationQuery, HistorySessionId, SearchDirection, SearchQuery,
        },
        painting::{Painter, PromptLines},
        prompt::{PromptEditMode, PromptHistorySearchStatus},
        utils::text_manipulation,
        EditCommand, ExampleHighlighter, Highlighter, LineBuffer, Menu, MenuEvent, Prompt,
        PromptHistorySearch, ReedlineMenu, Signal, ValidationResult, Validator,
    },
    crossterm::{
        event,
        event::{Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, Result,
    },
    std::{borrow::Borrow, fs::File, io, io::Write, process::Command, time::Duration},
};

// The POLL_WAIT is used to specify for how long the POLL should wait for
// events, to accelerate the handling of paste or compound resize events. Having
// a POLL_WAIT of zero means that every single event is treated as soon as it
// arrives. This doesn't allow for the possibility of more than 1 event
// happening at the same time.
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
/// use reedline::{Reedline, Signal, DefaultPrompt};
/// let mut line_editor = Reedline::create();
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
/// ```
pub struct Reedline {
    editor: Editor,

    // History
    history: Box<dyn History>,
    history_cursor: HistoryCursor,
    history_session_id: Option<HistorySessionId>, // none if history doesn't support this
    history_last_run_id: Option<HistoryItemId>,
    input_mode: InputMode,

    // Validator
    validator: Option<Box<dyn Validator>>,

    // Stdout
    painter: Painter,

    // Edit Mode: Vi, Emacs
    edit_mode: Box<dyn EditMode>,

    // Provides the tab completions
    completer: Box<dyn Completer>,
    quick_completions: bool,
    partial_completions: bool,

    // Performs bash style circular rotation through the available completions
    circular_completion_handler: CircularCompletionHandler,

    // Highlight the edit buffer
    highlighter: Box<dyn Highlighter>,

    // Showcase hints based on various strategies (history, language-completion, spellcheck, etc)
    hinter: Option<Box<dyn Hinter>>,
    hide_hints: bool,

    // Is Some(n) read_line() should repaint prompt every `n` milliseconds
    animate: bool,

    // Use ansi coloring or not
    use_ansi_coloring: bool,

    // Engine Menus
    menus: Vec<ReedlineMenu>,

    // Text editor used to open the line buffer for editing
    buffer_editor: Option<BufferEditor>,
}

struct BufferEditor {
    editor: String,
    extension: String,
}

impl Drop for Reedline {
    fn drop(&mut self) {
        // Ensures that the terminal is in a good state if we panic semigracefully
        // Calling `disable_raw_mode()` twice is fine with Linux
        let _ignore = terminal::disable_raw_mode();
    }
}

impl Reedline {
    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    #[must_use]
    pub fn create() -> Self {
        let history = Box::new(FileBackedHistory::default());
        let painter = Painter::new(std::io::BufWriter::new(std::io::stderr()));
        let buffer_highlighter = Box::new(ExampleHighlighter::default());
        let completer = Box::new(DefaultCompleter::default());
        let hinter = None;
        let validator = None;
        let edit_mode = Box::new(Emacs::default());

        Reedline {
            editor: Editor::default(),
            history,
            history_cursor: HistoryCursor::new(HistoryNavigationQuery::Normal(
                LineBuffer::default(),
            )),
            history_session_id: None,
            history_last_run_id: None,
            input_mode: InputMode::Regular,
            painter,
            edit_mode,
            completer,
            quick_completions: false,
            partial_completions: false,
            circular_completion_handler: CircularCompletionHandler::default(),
            highlighter: buffer_highlighter,
            hinter,
            hide_hints: false,
            validator,
            animate: false,
            use_ansi_coloring: true,
            menus: Vec::new(),
            buffer_editor: None,
        }
    }

    /// A builder to include a [`Hinter`] in your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// //Cargo.toml
    /// //[dependencies]
    /// //nu-ansi-term = "*"
    /// use {
    ///     nu_ansi_term::{Color, Style},
    ///     reedline::{DefaultHinter, Reedline},
    /// };
    ///
    /// let mut line_editor = Reedline::create().with_hinter(Box::new(
    ///     DefaultHinter::default()
    ///     .with_style(Style::new().italic().fg(Color::LightGray)),
    /// ));
    /// ```
    #[must_use]
    pub fn with_hinter(mut self, hinter: Box<dyn Hinter>) -> Self {
        self.hinter = Some(hinter);
        self
    }

    /// Remove current [`Hinter`]
    #[must_use]
    pub fn disable_hints(mut self) -> Self {
        self.hinter = None;
        self
    }

    /// A builder to configure the tab completion
    /// # Example
    /// ```rust
    /// // Create a reedline object with tab completions support
    ///
    /// use reedline::{DefaultCompleter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    ///
    /// let mut line_editor = Reedline::create().with_completer(completer);
    /// ```
    #[must_use]
    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> Self {
        self.completer = completer;
        self
    }

    /// Turn on quick completions. These completions will auto-select if the completer
    /// ever narrows down to a single entry.
    #[must_use]
    pub fn with_quick_completions(mut self, quick_completions: bool) -> Self {
        self.quick_completions = quick_completions;
        self
    }

    /// Turn on partial completions. These completions will fill the buffer with the
    /// smallest common string from all the options
    #[must_use]
    pub fn with_partial_completions(mut self, partial_completions: bool) -> Self {
        self.partial_completions = partial_completions;
        self
    }

    /// A builder which enables or disables the use of ansi coloring in the prompt
    /// and in the command line syntax highlighting.
    #[must_use]
    pub fn with_ansi_colors(mut self, use_ansi_coloring: bool) -> Self {
        self.use_ansi_coloring = use_ansi_coloring;
        self
    }

    /// A builder which enables or disables animations/automatic repainting of prompt.
    /// If `repaint` is true, every second the prompt will be repainted and the clock updates
    #[must_use]
    pub fn with_animation(mut self, repaint: bool) -> Self {
        self.animate = repaint;
        self
    }

    /// A builder that configures the highlighter for your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// // Create a reedline object with highlighter support
    ///
    /// use reedline::{ExampleHighlighter, Reedline};
    ///
    /// let commands = vec![
    ///   "test".into(),
    ///   "hello world".into(),
    ///   "hello world reedline".into(),
    ///   "this is the reedline crate".into(),
    /// ];
    /// let mut line_editor =
    /// Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
    /// ```
    #[must_use]
    pub fn with_highlighter(mut self, highlighter: Box<dyn Highlighter>) -> Self {
        self.highlighter = highlighter;
        self
    }

    /// A builder which configures the history for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with history support, including history size limits
    ///
    /// use reedline::{FileBackedHistory, Reedline};
    ///
    /// let history = Box::new(
    /// FileBackedHistory::with_file(5, "history.txt".into())
    ///     .expect("Error configuring history with file"),
    /// );
    /// let mut line_editor = Reedline::create()
    ///     .with_history(history);
    /// ```
    #[must_use]
    pub fn with_history(mut self, history: Box<dyn History>) -> Self {
        self.history = history;
        self
    }

    /// A builder that configures the validator for your instance of the Reedline engine
    /// # Example
    /// ```rust
    /// // Create a reedline object with validator support
    ///
    /// use reedline::{DefaultValidator, Reedline};
    ///
    /// let mut line_editor =
    /// Reedline::create().with_validator(Box::new(DefaultValidator));
    /// ```
    #[must_use]
    pub fn with_validator(mut self, validator: Box<dyn Validator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// A builder that configures the text editor used to edit the line buffer
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with vim as editor
    ///
    /// use reedline::{DefaultValidator, Reedline};
    ///
    /// let mut line_editor =
    /// Reedline::create().with_buffer_editor("vim".into(), "nu".into());
    /// ```
    #[must_use]
    pub fn with_buffer_editor(mut self, editor: String, extension: String) -> Self {
        self.buffer_editor = Some(BufferEditor { editor, extension });
        self
    }

    /// Remove the current [`Validator`]
    #[must_use]
    pub fn disable_validator(mut self) -> Self {
        self.validator = None;
        self
    }

    /// A builder which configures the edit mode for your instance of the Reedline engine
    #[must_use]
    pub fn with_edit_mode(mut self, edit_mode: Box<dyn EditMode>) -> Self {
        self.edit_mode = edit_mode;
        self
    }

    /// A builder that appends a menu to the engine
    #[must_use]
    pub fn with_menu(mut self, menu: ReedlineMenu) -> Self {
        self.menus.push(menu);
        self
    }

    /// A builder that clears the list of menus added to the engine
    #[must_use]
    pub fn clear_menus(mut self) -> Self {
        self.menus = Vec::new();
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
            .search(SearchQuery::everything(SearchDirection::Forward))
            .expect("todo: error handling");

        for (i, entry) in history.iter().enumerate() {
            self.print_line(&format!("{}\t{}", i, entry.command_line))?;
        }
        Ok(())
    }

    /// Read-only view of the history
    pub fn history(&self) -> &dyn History {
        &*self.history
    }

    /// Update the underlying [`History`] to/from disk
    pub fn sync_history(&mut self) -> std::io::Result<()> {
        // TODO: check for interactions in the non-submitting events
        self.history.sync()
    }

    /// update the last history item with more information
    pub fn update_last_command_context(
        &mut self,
        f: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> crate::Result<()> {
        if let Some(r) = &self.history_last_run_id {
            self.history.update(*r, f)?;
        } else {
            return Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                "No command run",
            )));
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

    /// Clear the screen and the scollback buffer of the terminal
    pub fn clear_scrollback(&mut self) -> Result<()> {
        self.painter.clear_scrollback()?;

        Ok(())
    }

    /// Helper implementing the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        self.painter.initialize_prompt_position()?;
        self.hide_hints = false;

        self.repaint(prompt)?;

        let mut crossterm_events: Vec<Event> = vec![];
        let mut reedline_events: Vec<ReedlineEvent> = vec![];

        loop {
            let mut paste_enter_state = false;

            if event::poll(Duration::from_millis(1000))? {
                let mut latest_resize = None;

                // There could be multiple events queued up!
                // pasting text, resizes, blocking this thread (e.g. during debugging)
                // We should be able to handle all of them as quickly as possible without causing unnecessary output steps.
                while event::poll(Duration::from_millis(POLL_WAIT))? {
                    match event::read()? {
                        Event::Resize(x, y) => {
                            latest_resize = Some((x, y));
                        }
                        enter @ Event::Key(KeyEvent {
                            code: KeyCode::Enter,
                            modifiers: KeyModifiers::NONE,
                        }) => {
                            crossterm_events.push(enter);
                            // Break early to check if the input is complete and
                            // can be send to the hosting application. If
                            // multiple complete entries are submitted, events
                            // are still in the crossterm queue for us to
                            // process.
                            paste_enter_state = crossterm_events.len() > EVENTS_THRESHOLD;
                            break;
                        }
                        x => {
                            crossterm_events.push(x);
                        }
                    }
                }

                if let Some((x, y)) = latest_resize {
                    reedline_events.push(ReedlineEvent::Resize(x, y));
                }

                // Accelerate pasted text by fusing `EditCommand`s
                //
                // (Text should only be `EditCommand::InsertChar`s)
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
            } else if self.animate && !self.painter.exceeds_screen_size() {
                reedline_events.push(ReedlineEvent::Repaint);
            };

            for event in reedline_events.drain(..) {
                match self.handle_event(prompt, event)? {
                    EventStatus::Exits(signal) => {
                        // Move the cursor below the input area, for external commands or new read_line call
                        self.painter.move_cursor_to_end()?;
                        return Ok(signal);
                    }
                    EventStatus::Handled => {
                        if !paste_enter_state {
                            self.repaint(prompt)?;
                        }
                    }
                    EventStatus::Inapplicable => {
                        // Nothing changed, no need to repaint
                    }
                }
            }
        }
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
            ReedlineEvent::UntilFound(events) => {
                for event in events {
                    match self.handle_history_search_event(prompt, event)? {
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
            ReedlineEvent::ClearScreen => {
                self.painter.clear_screen()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ClearScrollback => {
                self.painter.clear_scrollback()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Enter | ReedlineEvent::HistoryHintComplete => {
                if let Some(string) = self.history_cursor.string_at_cursor() {
                    self.editor.set_buffer(string);
                    self.editor.remember_undo_state(true);
                }

                self.input_mode = InputMode::Regular;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ExecuteHostCommand(host_command) => {
                // TODO: Decide if we need to do something special to have a nicer painter state on the next go
                Ok(EventStatus::Exits(Signal::Success(host_command)))
            }
            ReedlineEvent::Edit(commands) => {
                self.run_history_commands(&commands);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Mouse => Ok(EventStatus::Handled),
            ReedlineEvent::Resize(width, height) => {
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                // A handled Event causes a repaint
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::PreviousHistory | ReedlineEvent::Up | ReedlineEvent::SearchHistory => {
                self.history_cursor
                    .back(self.history.as_ref())
                    .expect("todo: error handling");
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory | ReedlineEvent::Down => {
                self.history_cursor
                    .forward(self.history.as_ref())
                    .expect("todo: error handling");
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history_cursor.string_at_cursor().is_none() {
                    self.history_cursor
                        .back(self.history.as_ref())
                        .expect("todo: error handling");
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Esc => {
                self.input_mode = InputMode::Regular;
                Ok(EventStatus::Handled)
            }
            // TODO: Check if events should be handled
            ReedlineEvent::Right
            | ReedlineEvent::Left
            | ReedlineEvent::ActionHandler
            | ReedlineEvent::Multiple(_)
            | ReedlineEvent::None
            | ReedlineEvent::HistoryHintWordComplete
            | ReedlineEvent::OpenEditor
            | ReedlineEvent::Menu(_)
            | ReedlineEvent::MenuNext
            | ReedlineEvent::MenuPrevious
            | ReedlineEvent::MenuUp
            | ReedlineEvent::MenuDown
            | ReedlineEvent::MenuLeft
            | ReedlineEvent::MenuRight
            | ReedlineEvent::MenuPageNext
            | ReedlineEvent::MenuPagePrevious => Ok(EventStatus::Inapplicable),
        }
    }

    fn handle_editor_event(
        &mut self,
        prompt: &dyn Prompt,
        event: ReedlineEvent,
    ) -> io::Result<EventStatus> {
        match event {
            ReedlineEvent::Menu(name) => {
                if self.active_menu().is_none() {
                    if let Some(menu) = self.menus.iter_mut().find(|menu| menu.name() == name) {
                        menu.menu_event(MenuEvent::Activate(self.quick_completions));

                        if self.quick_completions && menu.can_quick_complete() {
                            menu.update_values(
                                self.editor.line_buffer(),
                                self.completer.as_mut(),
                                self.history.as_ref(),
                            );

                            if menu.get_values().len() == 1 {
                                return self.handle_editor_event(prompt, ReedlineEvent::Enter);
                            }
                        }

                        if self.partial_completions
                            && menu.can_partially_complete(
                                self.quick_completions,
                                self.editor.line_buffer(),
                                self.completer.as_mut(),
                                self.history.as_ref(),
                            )
                        {
                            return Ok(EventStatus::Handled);
                        }

                        return Ok(EventStatus::Handled);
                    }
                }
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::MenuNext => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::NextElement);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuPrevious => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::PreviousElement);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuUp => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::MoveUp);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuDown => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::MoveDown);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuLeft => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::MoveLeft);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuRight => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::MoveRight);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuPageNext => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::NextPage);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::MenuPagePrevious => {
                self.active_menu()
                    .map_or(Ok(EventStatus::Inapplicable), |menu| {
                        menu.menu_event(MenuEvent::PreviousPage);
                        Ok(EventStatus::Handled)
                    })
            }
            ReedlineEvent::HistoryHintComplete => {
                if let Some(hinter) = self.hinter.as_mut() {
                    let current_hint = hinter.complete_hint();
                    if self.hints_active()
                        && self.editor.is_cursor_at_buffer_end()
                        && !current_hint.is_empty()
                        && self.active_menu().is_none()
                    {
                        self.run_edit_commands(&[EditCommand::InsertString(current_hint)]);
                        return Ok(EventStatus::Handled);
                    }
                }
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::HistoryHintWordComplete => {
                if let Some(hinter) = self.hinter.as_mut() {
                    let current_hint_part = hinter.next_hint_token();
                    if self.hints_active()
                        && self.editor.is_cursor_at_buffer_end()
                        && !current_hint_part.is_empty()
                        && self.active_menu().is_none()
                    {
                        self.run_edit_commands(&[EditCommand::InsertString(current_hint_part)]);
                        return Ok(EventStatus::Handled);
                    }
                }
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::ActionHandler => {
                let line_buffer = self.editor.line_buffer();
                self.circular_completion_handler
                    .handle(self.completer.as_mut(), line_buffer);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Esc => {
                self.deactivate_menus();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.editor.reset_undo_stack();
                    Ok(EventStatus::Exits(Signal::CtrlD))
                } else {
                    self.run_edit_commands(&[EditCommand::Delete]);
                    Ok(EventStatus::Handled)
                }
            }
            ReedlineEvent::CtrlC => {
                self.deactivate_menus();
                self.run_edit_commands(&[EditCommand::Clear]);
                self.editor.reset_undo_stack();
                Ok(EventStatus::Exits(Signal::CtrlC))
            }
            ReedlineEvent::ClearScreen => {
                self.deactivate_menus();
                self.painter.clear_screen()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ClearScrollback => {
                self.deactivate_menus();
                self.painter.clear_scrollback()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Enter => {
                for menu in self.menus.iter_mut() {
                    if menu.is_active() {
                        menu.replace_in_buffer(self.editor.line_buffer());
                        menu.menu_event(MenuEvent::Deactivate);

                        return Ok(EventStatus::Handled);
                    }
                }

                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }

                let buffer = self.editor.get_buffer().to_string();
                match self.validator.as_mut().map(|v| v.validate(&buffer)) {
                    None | Some(ValidationResult::Complete) => {
                        self.hide_hints = true;
                        // Additional repaint to show the content without hints etc.
                        self.repaint(prompt)?;
                        let buf = self.editor.get_buffer();
                        if !buf.is_empty() {
                            let mut entry = HistoryItem::from_command_line(buf);
                            // todo: in theory there's a race condition here because another shell might get the next session id at the same time
                            entry.session_id =
                                Some(*self.history_session_id.get_or_insert_with(|| {
                                    self.history
                                        .next_session_id()
                                        .expect("todo: error handling")
                                }));
                            let entry = self.history.save(entry).expect("todo: error handling");
                            self.history_last_run_id = entry.id;
                        }
                        self.run_edit_commands(&[EditCommand::Clear]);
                        self.editor.reset_undo_stack();

                        Ok(EventStatus::Exits(Signal::Success(buffer)))
                    }
                    Some(ValidationResult::Incomplete) => {
                        self.run_edit_commands(&[EditCommand::InsertNewline]);

                        Ok(EventStatus::Handled)
                    }
                }
            }
            ReedlineEvent::ExecuteHostCommand(host_command) => {
                // TODO: Decide if we need to do something special to have a nicer painter state on the next go
                Ok(EventStatus::Exits(Signal::Success(host_command)))
            }
            ReedlineEvent::Edit(commands) => {
                self.run_edit_commands(&commands);
                if let Some(menu) = self.menus.iter_mut().find(|men| men.is_active()) {
                    if self.quick_completions && menu.can_quick_complete() {
                        menu.menu_event(MenuEvent::Edit(self.quick_completions));
                        menu.update_values(
                            self.editor.line_buffer(),
                            self.completer.as_mut(),
                            self.history.as_ref(),
                        );

                        if menu.get_values().len() == 1 {
                            return self.handle_editor_event(prompt, ReedlineEvent::Enter);
                        }
                    }

                    if self.editor.line_buffer().get_buffer().is_empty() {
                        menu.menu_event(MenuEvent::Deactivate);
                    } else {
                        menu.menu_event(MenuEvent::Edit(self.quick_completions));
                    }
                }

                Ok(EventStatus::Handled)
            }
            ReedlineEvent::OpenEditor => self.open_editor().map(|_| EventStatus::Handled),
            ReedlineEvent::Resize(width, height) => {
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                // A handled Event causes a repaint
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::PreviousHistory => {
                self.previous_history();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory => {
                self.next_history();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Up => {
                self.up_command();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Down => {
                self.down_command();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Left => {
                self.run_edit_commands(&[EditCommand::MoveLeft]);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Right => {
                self.run_edit_commands(&[EditCommand::MoveRight]);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::SearchHistory => {
                // Make sure we are able to undo the result of a reverse history search
                self.editor.remember_undo_state(true);

                self.enter_history_search();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Multiple(events) => {
                let mut latest_signal = EventStatus::Inapplicable;
                for event in events {
                    match self.handle_editor_event(prompt, event)? {
                        EventStatus::Handled => {
                            latest_signal = EventStatus::Handled;
                        }
                        EventStatus::Inapplicable => {
                            // NO OP
                        }
                        EventStatus::Exits(signal) => {
                            // TODO: Check if we want to allow execution to
                            // proceed if there are more events after the
                            // terminating
                            return Ok(EventStatus::Exits(signal));
                        }
                    }
                }

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
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::None | ReedlineEvent::Mouse => Ok(EventStatus::Inapplicable),
        }
    }

    fn active_menu(&mut self) -> Option<&mut ReedlineMenu> {
        self.menus.iter_mut().find(|menu| menu.is_active())
    }

    fn deactivate_menus(&mut self) {
        self.menus
            .iter_mut()
            .for_each(|menu| menu.menu_event(MenuEvent::Deactivate));
    }

    fn previous_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.history_cursor =
                HistoryCursor::new(self.get_history_navigation_based_on_line_buffer());
        }

        self.history_cursor
            .back(self.history.as_ref())
            .expect("todo: error handling");
        self.update_buffer_from_history();
        self.editor.move_to_start();
        self.editor.move_to_line_end();
    }

    fn next_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.history_cursor =
                HistoryCursor::new(self.get_history_navigation_based_on_line_buffer());
        }

        self.history_cursor
            .forward(self.history.as_ref())
            .expect("todo: error handling");
        self.update_buffer_from_history();
        self.editor.move_to_end();
    }

    /// Enable the search and navigation through the history from the line buffer prompt
    ///
    /// Enables either prefix search with output in the line buffer or simple traversal
    fn get_history_navigation_based_on_line_buffer(&self) -> HistoryNavigationQuery {
        if self.editor.is_empty() || !self.editor.is_cursor_at_buffer_end() {
            // Perform bash-style basic up/down entry walking
            HistoryNavigationQuery::Normal(
                // Hack: Tight coupling point to be able to restore previously typed input
                self.editor.line_buffer_immut().clone(),
            )
        } else {
            // Prefix search like found in fish, zsh, etc.
            // Search string is set once from the current buffer
            // Current setup (code in other methods)
            // Continuing with typing will leave the search
            // but next invocation of this method will start the next search
            let buffer = self.editor.get_buffer().to_string();
            HistoryNavigationQuery::PrefixSearch(buffer)
        }
    }

    /// Switch into reverse history search mode
    ///
    /// This mode uses a separate prompt and handles keybindings slightly differently!
    fn enter_history_search(&mut self) {
        self.history_cursor =
            HistoryCursor::new(HistoryNavigationQuery::SubstringSearch("".to_string()));
        self.input_mode = InputMode::HistorySearch;
    }

    /// Dispatches the applicable [`EditCommand`] actions for editing the history search string.
    ///
    /// Only modifies internal state, does not perform regular output!
    fn run_history_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::InsertChar(c) => {
                    let navigation = self.history_cursor.get_navigation();
                    if let HistoryNavigationQuery::SubstringSearch(mut substring) = navigation {
                        substring.push(*c);
                        self.history_cursor =
                            HistoryCursor::new(HistoryNavigationQuery::SubstringSearch(substring));
                    } else {
                        self.history_cursor = HistoryCursor::new(
                            HistoryNavigationQuery::SubstringSearch(String::from(*c)),
                        );
                    }
                    self.history_cursor
                        .back(self.history.as_mut())
                        .expect("todo: error handling");
                }
                EditCommand::Backspace => {
                    let navigation = self.history_cursor.get_navigation();

                    if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
                        let new_substring = text_manipulation::remove_last_grapheme(&substring);

                        self.history_cursor = HistoryCursor::new(
                            HistoryNavigationQuery::SubstringSearch(new_substring.to_string()),
                        );
                        self.history_cursor
                            .back(self.history.as_mut())
                            .expect("todo: error handling");
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
        match self.history_cursor.get_navigation() {
            HistoryNavigationQuery::Normal(original) => {
                if let Some(buffer_to_paint) = self.history_cursor.string_at_cursor() {
                    self.editor.set_buffer(buffer_to_paint.clone());
                    self.editor.set_insertion_point(buffer_to_paint.len());
                } else {
                    // Hack
                    self.editor.set_line_buffer(original);
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                if let Some(prefix_result) = self.history_cursor.string_at_cursor() {
                    self.editor.set_buffer(prefix_result.clone());
                    self.editor.set_insertion_point(prefix_result.len());
                } else {
                    self.editor.set_buffer(prefix.clone());
                    self.editor.set_insertion_point(prefix.len());
                }
            }
            HistoryNavigationQuery::SubstringSearch(_) => todo!(),
        }
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        if self.input_mode == InputMode::HistoryTraversal {
            if matches!(
                self.history_cursor.get_navigation(),
                HistoryNavigationQuery::Normal(_)
            ) {
                if let Some(string) = self.history_cursor.string_at_cursor() {
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
        !self.hide_hints && matches!(self.input_mode, InputMode::Regular)
    }

    /// Repaint of either the buffer or the parts for reverse history search
    fn repaint(&mut self, prompt: &dyn Prompt) -> io::Result<()> {
        // Repainting
        if self.input_mode == InputMode::HistorySearch {
            self.history_search_paint(prompt)
        } else {
            self.buffer_paint(prompt)
        }
    }

    #[cfg(feature = "bashisms")]
    /// Parses the ! command to replace entries from the history
    fn parse_bang_command(&mut self) -> Option<ReedlineEvent> {
        let buffer = self.editor.get_buffer();
        let parsed = parse_selection_char(buffer, '!');

        if let Some(last) = parsed.remainder.chars().last() {
            if last != ' ' {
                return None;
            }
        }

        let history_result = parsed
            .index
            .zip(parsed.marker)
            .and_then(|(index, indicator)| match parsed.action {
                ParseAction::LastCommand => self
                    .history
                    .search(SearchQuery {
                        direction: SearchDirection::Backward,
                        start_time: None,
                        end_time: None,
                        start_id: None,
                        end_id: None,
                        limit: Some(1), // fetch the latest one entries
                        filter: SearchFilter::anything(),
                    })
                    .unwrap_or_else(|_| Vec::new())
                    .get(index.saturating_sub(1))
                    .map(|history| {
                        (
                            parsed.remainder.len(),
                            indicator.len(),
                            history.command_line.clone(),
                        )
                    }),
                ParseAction::BackwardSearch => self
                    .history
                    .search(SearchQuery {
                        direction: SearchDirection::Backward,
                        start_time: None,
                        end_time: None,
                        start_id: None,
                        end_id: None,
                        limit: Some(index as i64), // fetch the latest n entries
                        filter: SearchFilter::anything(),
                    })
                    .unwrap_or_else(|_| Vec::new())
                    .get(index.saturating_sub(1))
                    .map(|history| {
                        (
                            parsed.remainder.len(),
                            indicator.len(),
                            history.command_line.clone(),
                        )
                    }),
                ParseAction::ForwardSearch => self
                    .history
                    .search(SearchQuery {
                        direction: SearchDirection::Forward,
                        start_time: None,
                        end_time: None,
                        start_id: None,
                        end_id: None,
                        limit: Some((index + 1) as i64), // fetch the oldest n entries
                        filter: SearchFilter::anything(),
                    })
                    .unwrap_or_else(|_| Vec::new())
                    .get(index)
                    .map(|history| {
                        (
                            parsed.remainder.len(),
                            indicator.len(),
                            history.command_line.clone(),
                        )
                    }),
                ParseAction::LastToken => self
                    .history
                    .search(SearchQuery::last_with_search(SearchFilter::anything()))
                    .unwrap_or_else(|_| Vec::new())
                    .get(0)
                    .and_then(|history| history.command_line.split_whitespace().rev().next())
                    .map(|token| (parsed.remainder.len(), indicator.len(), token.to_string())),
            });

        if let Some((start, size, history)) = history_result {
            let edits = vec![
                EditCommand::MoveToPosition(start),
                EditCommand::ReplaceChars(size, history),
            ];

            Some(ReedlineEvent::Edit(edits))
        } else {
            None
        }
    }

    fn open_editor(&mut self) -> Result<()> {
        match &self.buffer_editor {
            None => Ok(()),
            Some(BufferEditor { editor, extension }) => {
                let temp_directory = std::env::temp_dir();
                let temp_file = temp_directory.join(format!("reedline_buffer.{}", extension));

                {
                    let mut file = File::create(temp_file.clone())?;
                    write!(file, "{}", self.editor.get_buffer())?;
                }

                {
                    let mut process = Command::new(editor);
                    process.arg(temp_file.as_path());

                    let mut child = process.spawn()?;
                    child.wait()?;
                }

                let res = std::fs::read_to_string(temp_file)?;
                let res = res.trim_end().to_string();

                self.editor.line_buffer().set_buffer(res);

                Ok(())
            }
        }
    }

    /// Repaint logic for the history reverse search
    ///
    /// Overwrites the prompt indicator and highlights the search string
    /// separately from the result buffer.
    fn history_search_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let navigation = self.history_cursor.get_navigation();

        if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
            let status =
                if !substring.is_empty() && self.history_cursor.string_at_cursor().is_none() {
                    PromptHistorySearchStatus::Failing
                } else {
                    PromptHistorySearchStatus::Passing
                };

            let prompt_history_search = PromptHistorySearch::new(status, substring.clone());

            let res_string = self.history_cursor.string_at_cursor().unwrap_or_default();

            // Highlight matches
            let res_string = if self.use_ansi_coloring {
                let match_highlighter = SimpleMatchHighlighter::new(substring);
                let styled = match_highlighter.highlight(&res_string, 0);
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
                .repaint_buffer(prompt, &lines, None, self.use_ansi_coloring)?;
        }

        Ok(())
    }

    /// Triggers a full repaint including the prompt parts
    ///
    /// Includes the highlighting and hinting calls.
    fn buffer_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let cursor_position_in_buffer = self.editor.insertion_point();
        let buffer_to_paint = self.editor.get_buffer();

        let (before_cursor, after_cursor) = self
            .highlighter
            .highlight(buffer_to_paint, cursor_position_in_buffer)
            .render_around_insertion_point(
                cursor_position_in_buffer,
                prompt.render_prompt_multiline_indicator().borrow(),
                self.use_ansi_coloring,
            );

        let hint: String = if self.hints_active() {
            self.hinter.as_mut().map_or_else(String::new, |hinter| {
                hinter.handle(
                    buffer_to_paint,
                    cursor_position_in_buffer,
                    self.history.as_ref(),
                    self.use_ansi_coloring,
                )
            })
        } else {
            String::new()
        };

        // Needs to add return carriage to newlines because when not in raw mode
        // some OS don't fully return the carriage

        let lines = PromptLines::new(
            prompt,
            self.prompt_edit_mode(),
            None,
            &before_cursor,
            &after_cursor,
            &hint,
        );

        // Updating the working details of the active menu
        for menu in self.menus.iter_mut() {
            if menu.is_active() {
                menu.update_working_details(
                    self.editor.line_buffer(),
                    self.completer.as_mut(),
                    self.history.as_ref(),
                    &self.painter,
                );
            }
        }

        let menu = self.menus.iter().find(|menu| menu.is_active());

        self.painter
            .repaint_buffer(prompt, &lines, menu, self.use_ansi_coloring)
    }
}

#[test]
fn thread_safe() {
    fn f<S: Send>(_: S) {}
    f(Reedline::create());
}
