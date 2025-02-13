use std::path::PathBuf;

use itertools::Itertools;
use nu_ansi_term::{Color, Style};

use crate::{enums::ReedlineRawEvent, CursorConfig};
#[cfg(feature = "bashisms")]
use crate::{
    history::SearchFilter,
    menu_functions::{parse_selection_char, ParseAction},
};
#[cfg(feature = "external_printer")]
use {
    crate::external_printer::ExternalPrinter,
    crossbeam::channel::TryRecvError,
    std::io::{Error, ErrorKind},
};
use {
    crate::{
        completion::{Completer, DefaultCompleter},
        core_editor::Editor,
        edit_mode::{EditMode, Emacs},
        enums::{EventStatus, ReedlineEvent},
        highlighter::SimpleMatchHighlighter,
        hinter::Hinter,
        history::{
            FileBackedHistory, History, HistoryCursor, HistoryItem, HistoryItemId,
            HistoryNavigationQuery, HistorySessionId, SearchDirection, SearchQuery,
        },
        painting::{Painter, PainterSuspendedState, PromptLines},
        prompt::{PromptEditMode, PromptHistorySearchStatus},
        result::{ReedlineError, ReedlineErrorVariants},
        terminal_extensions::{bracketed_paste::BracketedPasteGuard, kitty::KittyProtocolGuard},
        utils::text_manipulation,
        EditCommand, ExampleHighlighter, Highlighter, LineBuffer, Menu, MenuEvent, Prompt,
        PromptHistorySearch, ReedlineMenu, Signal, UndoBehavior, ValidationResult, Validator,
    },
    crossterm::{
        cursor::{SetCursorStyle, Show},
        event,
        event::{Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, QueueableCommand,
    },
    std::{
        fs::File, io, io::Result, io::Write, process::Command, time::Duration, time::SystemTime,
    },
};

// The POLL_WAIT is used to specify for how long the POLL should wait for
// events, to accelerate the handling of paste or compound resize events. Having
// a POLL_WAIT of zero means that every single event is treated as soon as it
// arrives. This doesn't allow for the possibility of more than 1 event
// happening at the same time.
const POLL_WAIT: Duration = Duration::from_millis(100);
// Since a paste event is multiple `Event::Key` events happening at the same
// time, we specify how many events should be in the `crossterm_events` vector
// before it is considered a paste. 10 events is conservative enough.
const EVENTS_THRESHOLD: usize = 10;

/// Maximum time Reedline will block on input before yielding control to
/// external printers.
#[cfg(feature = "external_printer")]
const EXTERNAL_PRINTER_WAIT: Duration = Duration::from_millis(100);

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
    history_session_id: Option<HistorySessionId>,
    // none if history doesn't support this
    history_last_run_id: Option<HistoryItemId>,
    history_exclusion_prefix: Option<String>,
    history_excluded_item: Option<HistoryItem>,
    history_cursor_on_excluded: bool,
    input_mode: InputMode,

    // State of the painter after a `ReedlineEvent::ExecuteHostCommand` was requested, used after
    // execution to decide if we can re-use the previous prompt or paint a new one.
    suspended_state: Option<PainterSuspendedState>,

    // Validator
    validator: Option<Box<dyn Validator>>,

    // Stdout
    painter: Painter,

    transient_prompt: Option<Box<dyn Prompt>>,

    // Edit Mode: Vi, Emacs
    edit_mode: Box<dyn EditMode>,

    // Provides the tab completions
    completer: Box<dyn Completer>,
    quick_completions: bool,
    partial_completions: bool,

    // Highlight the edit buffer
    highlighter: Box<dyn Highlighter>,

    // Style used for visual selection
    visual_selection_style: Style,

    // Showcase hints based on various strategies (history, language-completion, spellcheck, etc)
    hinter: Option<Box<dyn Hinter>>,
    hide_hints: bool,

    // Use ansi coloring or not
    use_ansi_coloring: bool,

    // Current working directory as defined by the application. If set, it will
    // override the actual working directory of the process.
    cwd: Option<String>,

    // Engine Menus
    menus: Vec<ReedlineMenu>,

    // Text editor used to open the line buffer for editing
    buffer_editor: Option<BufferEditor>,

    // Use different cursors depending on the current edit mode
    cursor_shapes: Option<CursorConfig>,

    // Manage bracketed paste mode
    bracketed_paste: BracketedPasteGuard,

    // Manage optional kitty protocol
    kitty_protocol: KittyProtocolGuard,

    #[cfg(feature = "external_printer")]
    external_printer: Option<ExternalPrinter<String>>,
}

struct BufferEditor {
    command: Command,
    temp_file: PathBuf,
}

impl Drop for Reedline {
    fn drop(&mut self) {
        if self.cursor_shapes.is_some() {
            let _ignore = terminal::enable_raw_mode();
            let mut stdout = std::io::stdout();
            let _ignore = stdout.queue(SetCursorStyle::DefaultUserShape);
            let _ignore = stdout.queue(Show);
            let _ignore = stdout.flush();
        }

        // Ensures that the terminal is in a good state if we panic semigracefully
        // Calling `disable_raw_mode()` twice is fine with Linux
        let _ignore = terminal::disable_raw_mode();
    }
}

impl Reedline {
    const FILTERED_ITEM_ID: HistoryItemId = HistoryItemId(i64::MAX);

    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    #[must_use]
    pub fn create() -> Self {
        let history = Box::<FileBackedHistory>::default();
        let painter = Painter::new(std::io::BufWriter::new(std::io::stderr()));
        let buffer_highlighter = Box::<ExampleHighlighter>::default();
        let visual_selection_style = Style::new().on(Color::LightGray);
        let completer = Box::<DefaultCompleter>::default();
        let hinter = None;
        let validator = None;
        let edit_mode = Box::<Emacs>::default();
        let hist_session_id = None;

        Reedline {
            editor: Editor::default(),
            history,
            history_cursor: HistoryCursor::new(
                HistoryNavigationQuery::Normal(LineBuffer::default()),
                hist_session_id,
            ),
            history_session_id: hist_session_id,
            history_last_run_id: None,
            history_exclusion_prefix: None,
            history_excluded_item: None,
            history_cursor_on_excluded: false,
            input_mode: InputMode::Regular,
            suspended_state: None,
            painter,
            transient_prompt: None,
            edit_mode,
            completer,
            quick_completions: false,
            partial_completions: false,
            highlighter: buffer_highlighter,
            visual_selection_style,
            hinter,
            hide_hints: false,
            validator,
            use_ansi_coloring: true,
            cwd: None,
            menus: Vec::new(),
            buffer_editor: None,
            cursor_shapes: None,
            bracketed_paste: BracketedPasteGuard::default(),
            kitty_protocol: KittyProtocolGuard::default(),
            #[cfg(feature = "external_printer")]
            external_printer: None,
        }
    }

    /// Get a new history session id based on the current time and the first commit datetime of reedline
    pub fn create_history_session_id() -> Option<HistorySessionId> {
        let nanos = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => n.as_nanos() as i64,
            Err(_) => 0,
        };

        Some(HistorySessionId::new(nanos))
    }

    /// Toggle whether reedline enables bracketed paste to reed copied content
    ///
    /// This currently alters the behavior for multiline pastes as pasting of regular text will
    /// execute after every complete new line as determined by the [`Validator`]. With enabled
    /// bracketed paste all lines will appear in the buffer and can then be submitted with a
    /// separate enter.
    ///
    /// At this point most terminals should support it or ignore the setting of the necessary
    /// flags. For full compatibility, keep it disabled.
    pub fn use_bracketed_paste(mut self, enable: bool) -> Self {
        self.bracketed_paste.set(enable);
        self
    }

    /// Toggle whether reedline uses the kitty keyboard enhancement protocol
    ///
    /// This allows us to disambiguate more events than the traditional standard
    /// Only available with a few terminal emulators.
    /// You can check for that with [`crate::kitty_protocol_available`]
    /// `Reedline` will perform this check internally
    ///
    /// Read more: <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
    pub fn use_kitty_keyboard_enhancement(mut self, enable: bool) -> Self {
        self.kitty_protocol.set(enable);
        self
    }

    /// Return the previously generated history session id
    pub fn get_history_session_id(&self) -> Option<HistorySessionId> {
        self.history_session_id
    }

    /// Set a new history session id
    /// This should be used in situations where the user initially did not have a history_session_id
    /// and then later realized they want to have one without restarting the application.
    pub fn set_history_session_id(&mut self, session: Option<HistorySessionId>) -> Result<()> {
        self.history_session_id = session;
        Ok(())
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

    /// Update current working directory.
    #[must_use]
    pub fn with_cwd(mut self, cwd: Option<String>) -> Self {
        self.cwd = cwd;
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

    /// A builder that configures the style used for visual selection
    #[must_use]
    pub fn with_visual_selection_style(mut self, style: Style) -> Self {
        self.visual_selection_style = style;
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

    /// A builder which configures history exclusion for your instance of the Reedline engine
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline instance with history that will *not* include commands starting with a space
    ///
    /// use reedline::{FileBackedHistory, Reedline};
    ///
    /// let history = Box::new(
    /// FileBackedHistory::with_file(5, "history.txt".into())
    ///     .expect("Error configuring history with file"),
    /// );
    /// let mut line_editor = Reedline::create()
    ///     .with_history(history)
    ///     .with_history_exclusion_prefix(Some(" ".into()));
    /// ```
    #[must_use]
    pub fn with_history_exclusion_prefix(mut self, ignore_prefix: Option<String>) -> Self {
        self.history_exclusion_prefix = ignore_prefix;
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

    /// A builder that configures the alternate text editor used to edit the line buffer
    ///
    /// You are responsible for providing a file path that is unique to this reedline session
    ///
    /// # Example
    /// ```rust,no_run
    /// // Create a reedline object with vim as editor
    ///
    /// use reedline::Reedline;
    /// use std::env::temp_dir;
    /// use std::process::Command;
    ///
    /// let temp_file = std::env::temp_dir().join("my-random-unique.file");
    /// let mut command = Command::new("vim");
    /// // you can provide additional flags:
    /// command.arg("-p"); // open in a vim tab (just for demonstration)
    /// // you don't have to pass the filename to the command
    /// let mut line_editor =
    /// Reedline::create().with_buffer_editor(command, temp_file);
    /// ```
    #[must_use]
    pub fn with_buffer_editor(mut self, editor: Command, temp_file: PathBuf) -> Self {
        let mut editor = editor;
        if !editor.get_args().contains(&temp_file.as_os_str()) {
            editor.arg(&temp_file);
        }
        self.buffer_editor = Some(BufferEditor {
            command: editor,
            temp_file,
        });
        self
    }

    /// Remove the current [`Validator`]
    #[must_use]
    pub fn disable_validator(mut self) -> Self {
        self.validator = None;
        self
    }

    /// Set a different prompt to be used after submitting each line
    #[must_use]
    pub fn with_transient_prompt(mut self, transient_prompt: Box<dyn Prompt>) -> Self {
        self.transient_prompt = Some(transient_prompt);
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

    /// A builder that adds the history item id
    #[must_use]
    pub fn with_history_session_id(mut self, session: Option<HistorySessionId>) -> Self {
        self.history_session_id = session;
        self
    }

    /// A builder that enables reedline changing the cursor shape based on the current edit mode.
    /// The current implementation sets the cursor shape when drawing the prompt.
    /// Do not use this if the cursor shape is set elsewhere, e.g. in the terminal settings or by ansi escape sequences.
    pub fn with_cursor_config(mut self, cursor_shapes: CursorConfig) -> Self {
        self.cursor_shapes = Some(cursor_shapes);
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
            .search(SearchQuery::everything(SearchDirection::Forward, None))
            .expect("todo: error handling");

        for (i, entry) in history.iter().enumerate() {
            self.print_line(&format!("{}\t{}", i, entry.command_line))?;
        }
        Ok(())
    }

    /// Output the complete [`History`] for this session, chronologically with numbering to the terminal
    pub fn print_history_session(&mut self) -> Result<()> {
        let history: Vec<_> = self
            .history
            .search(SearchQuery::everything(
                SearchDirection::Forward,
                self.get_history_session_id(),
            ))
            .expect("todo: error handling");

        for (i, entry) in history.iter().enumerate() {
            self.print_line(&format!("{}\t{}", i, entry.command_line))?;
        }
        Ok(())
    }

    /// Print the history session id
    pub fn print_history_session_id(&mut self) -> Result<()> {
        println!("History Session Id: {:?}", self.get_history_session_id());
        Ok(())
    }

    /// Toggle between having a history that uses the history session id and one that does not
    pub fn toggle_history_session_matching(
        &mut self,
        session: Option<HistorySessionId>,
    ) -> Result<()> {
        self.history_session_id = match self.get_history_session_id() {
            Some(_) => None,
            None => session,
        };
        Ok(())
    }

    /// Read-only view of the history
    pub fn history(&self) -> &dyn History {
        &*self.history
    }

    /// Mutable view of the history
    pub fn history_mut(&mut self) -> &mut dyn History {
        &mut *self.history
    }

    /// Update the underlying [`History`] to/from disk
    pub fn sync_history(&mut self) -> std::io::Result<()> {
        // TODO: check for interactions in the non-submitting events
        self.history.sync()
    }

    /// Check if any commands have been run.
    ///
    /// When no commands have been run, calling [`Self::update_last_command_context`]
    /// does not make sense and is guaranteed to fail with a "No command run" error.
    pub fn has_last_command_context(&self) -> bool {
        self.history_last_run_id.is_some()
    }

    /// update the last history item with more information
    pub fn update_last_command_context(
        &mut self,
        f: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> crate::Result<()> {
        match &self.history_last_run_id {
            Some(Self::FILTERED_ITEM_ID) => {
                self.history_excluded_item = Some(f(self.history_excluded_item.take().unwrap()));
                Ok(())
            }
            Some(r) => self.history.update(*r, f),
            None => Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                "No command run",
            ))),
        }
    }

    /// Wait for input and provide the user with a specified [`Prompt`].
    ///
    /// Returns a [`std::io::Result`] in which the `Err` type is [`std::io::Result`]
    /// and the `Ok` variant wraps a [`Signal`] which handles user inputs.
    pub fn read_line(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        terminal::enable_raw_mode()?;
        self.bracketed_paste.enter();
        self.kitty_protocol.enter();

        let result = self.read_line_helper(prompt);

        self.bracketed_paste.exit();
        self.kitty_protocol.exit();
        terminal::disable_raw_mode()?;
        result
    }

    /// Returns the current insertion point of the input buffer.
    pub fn current_insertion_point(&self) -> usize {
        self.editor.insertion_point()
    }

    /// Returns the current contents of the input buffer.
    pub fn current_buffer_contents(&self) -> &str {
        self.editor.get_buffer()
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

    /// Clear the screen and the scrollback buffer of the terminal
    pub fn clear_scrollback(&mut self) -> Result<()> {
        self.painter.clear_scrollback()?;

        Ok(())
    }

    /// Helper implementing the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        self.painter
            .initialize_prompt_position(self.suspended_state.as_ref())?;
        if self.suspended_state.is_some() {
            // Last editor was suspended to run a ExecuteHostCommand event,
            // we are resuming operation now.
            self.suspended_state = None;
        }
        self.hide_hints = false;

        self.repaint(prompt)?;

        loop {
            #[cfg(feature = "external_printer")]
            if let Some(ref external_printer) = self.external_printer {
                // get messages from printer as crlf separated "lines"
                let messages = Self::external_messages(external_printer)?;
                if !messages.is_empty() {
                    // print the message(s)
                    self.painter.print_external_message(
                        messages,
                        self.editor.line_buffer(),
                        prompt,
                    )?;
                    self.repaint(prompt)?;
                }
            }

            // Helper function that returns true if the input is complete and
            // can be sent to the hosting application.
            fn completed(events: &[Event]) -> bool {
                if let Some(event) = events.last() {
                    matches!(
                        event,
                        Event::Key(KeyEvent {
                            code: KeyCode::Enter,
                            modifiers: KeyModifiers::NONE,
                            ..
                        })
                    )
                } else {
                    false
                }
            }

            let mut events: Vec<Event> = vec![];

            // If the `external_printer` feature is enabled, we need to
            // periodically yield so that external printers get a chance to
            // print. Otherwise, we can just block until we receive an event.
            #[cfg(feature = "external_printer")]
            if event::poll(EXTERNAL_PRINTER_WAIT)? {
                events.push(crossterm::event::read()?);
            }
            #[cfg(not(feature = "external_printer"))]
            events.push(crossterm::event::read()?);

            // Receive all events in the queue without blocking. Will stop when
            // a line of input is completed.
            while !completed(&events) && event::poll(Duration::from_millis(0))? {
                events.push(crossterm::event::read()?);
            }

            // If we believe there's text pasting or resizing going on, batch
            // more events at the cost of a slight delay.
            if events.len() > EVENTS_THRESHOLD
                || events.iter().any(|e| matches!(e, Event::Resize(_, _)))
            {
                while !completed(&events) && event::poll(POLL_WAIT)? {
                    events.push(crossterm::event::read()?);
                }
            }

            // Convert `Event` into `ReedlineEvent`. Also, fuse consecutive
            // `ReedlineEvent::EditCommand` into one. Also, if there're multiple
            // `ReedlineEvent::Resize`, only keep the last one.
            let mut reedline_events: Vec<ReedlineEvent> = vec![];
            let mut edits = vec![];
            let mut resize = None;
            for event in events {
                if let Ok(event) = ReedlineRawEvent::try_from(event) {
                    match self.edit_mode.parse_event(event) {
                        ReedlineEvent::Edit(edit) => edits.extend(edit),
                        ReedlineEvent::Resize(x, y) => resize = Some((x, y)),
                        event => {
                            if !edits.is_empty() {
                                reedline_events
                                    .push(ReedlineEvent::Edit(std::mem::take(&mut edits)));
                            }
                            reedline_events.push(event);
                        }
                    }
                }
            }
            if !edits.is_empty() {
                reedline_events.push(ReedlineEvent::Edit(edits));
            }
            if let Some((x, y)) = resize {
                reedline_events.push(ReedlineEvent::Resize(x, y));
            }

            // Handle reedline events.
            for event in reedline_events {
                match self.handle_event(prompt, event)? {
                    EventStatus::Exits(signal) => {
                        // Check if we are merely suspended (to process an ExecuteHostCommand event)
                        // or if we're about to quit the editor.
                        if self.suspended_state.is_none() {
                            // We are about to quit the editor, move the cursor below the input
                            // area, for external commands or new read_line call
                            self.painter.move_cursor_to_end()?;
                        }
                        return Ok(signal);
                    }
                    EventStatus::Handled => {
                        self.repaint(prompt)?;
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
            self.handle_history_search_event(event)
        } else {
            self.handle_editor_event(prompt, event)
        }
    }

    fn handle_history_search_event(&mut self, event: ReedlineEvent) -> io::Result<EventStatus> {
        match event {
            ReedlineEvent::UntilFound(events) => {
                for event in events {
                    match self.handle_history_search_event(event)? {
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
            ReedlineEvent::Enter
            | ReedlineEvent::HistoryHintComplete
            | ReedlineEvent::Submit
            | ReedlineEvent::SubmitOrNewline => {
                if let Some(string) = self.history_cursor.string_at_cursor() {
                    self.editor
                        .set_buffer(string, UndoBehavior::CreateUndoPoint);
                }

                self.input_mode = InputMode::Regular;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ExecuteHostCommand(host_command) => {
                self.suspended_state = Some(self.painter.state_before_suspension());
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
                                &mut self.editor,
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
                                &mut self.editor,
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
                if let Some(menu) = self.menus.iter_mut().find(|menu| menu.is_active()) {
                    if menu.get_values().len() == 1 && menu.can_quick_complete() {
                        self.handle_editor_event(prompt, ReedlineEvent::Enter)
                    } else {
                        if self.partial_completions {
                            menu.can_partially_complete(
                                self.quick_completions,
                                &mut self.editor,
                                self.completer.as_mut(),
                                self.history.as_ref(),
                            );
                        }
                        menu.menu_event(MenuEvent::NextElement);
                        Ok(EventStatus::Handled)
                    }
                } else {
                    Ok(EventStatus::Inapplicable)
                }
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
            ReedlineEvent::Esc => {
                self.deactivate_menus();
                self.editor.reset_selection();
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
            ReedlineEvent::Enter | ReedlineEvent::Submit | ReedlineEvent::SubmitOrNewline
                if self.menus.iter().any(|menu| menu.is_active()) =>
            {
                for menu in self.menus.iter_mut() {
                    if menu.is_active() {
                        menu.replace_in_buffer(&mut self.editor);
                        menu.menu_event(MenuEvent::Deactivate);

                        return Ok(EventStatus::Handled);
                    }
                }
                unreachable!()
            }
            ReedlineEvent::Enter => {
                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }

                let buffer = self.editor.get_buffer().to_string();
                match self.validator.as_mut().map(|v| v.validate(&buffer)) {
                    None | Some(ValidationResult::Complete) => Ok(self.submit_buffer(prompt)?),
                    Some(ValidationResult::Incomplete) => {
                        self.run_edit_commands(&[EditCommand::InsertNewline]);

                        Ok(EventStatus::Handled)
                    }
                }
            }
            ReedlineEvent::Submit => {
                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }
                Ok(self.submit_buffer(prompt)?)
            }
            ReedlineEvent::SubmitOrNewline => {
                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }
                let cursor_position_in_buffer = self.editor.insertion_point();
                let buffer = self.editor.get_buffer().to_string();
                if cursor_position_in_buffer < buffer.len() {
                    self.run_edit_commands(&[EditCommand::InsertNewline]);
                    return Ok(EventStatus::Handled);
                }
                match self.validator.as_mut().map(|v| v.validate(&buffer)) {
                    None | Some(ValidationResult::Complete) => Ok(self.submit_buffer(prompt)?),
                    Some(ValidationResult::Incomplete) => {
                        self.run_edit_commands(&[EditCommand::InsertNewline]);

                        Ok(EventStatus::Handled)
                    }
                }
            }
            ReedlineEvent::ExecuteHostCommand(host_command) => {
                self.suspended_state = Some(self.painter.state_before_suspension());
                Ok(EventStatus::Exits(Signal::Success(host_command)))
            }
            ReedlineEvent::Edit(commands) => {
                self.run_edit_commands(&commands);
                if let Some(menu) = self.menus.iter_mut().find(|men| men.is_active()) {
                    if self.quick_completions && menu.can_quick_complete() {
                        match commands.first() {
                            Some(&EditCommand::Backspace)
                            | Some(&EditCommand::BackspaceWord)
                            | Some(&EditCommand::MoveToLineStart { select: false }) => {
                                menu.menu_event(MenuEvent::Deactivate)
                            }
                            _ => {
                                menu.menu_event(MenuEvent::Edit(self.quick_completions));
                                menu.update_values(
                                    &mut self.editor,
                                    self.completer.as_mut(),
                                    self.history.as_ref(),
                                );
                                if let Some(&EditCommand::Complete) = commands.first() {
                                    if menu.get_values().len() == 1 {
                                        return self
                                            .handle_editor_event(prompt, ReedlineEvent::Enter);
                                    } else if self.partial_completions
                                        && menu.can_partially_complete(
                                            self.quick_completions,
                                            &mut self.editor,
                                            self.completer.as_mut(),
                                            self.history.as_ref(),
                                        )
                                    {
                                        return Ok(EventStatus::Handled);
                                    }
                                }
                            }
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
                self.run_edit_commands(&[EditCommand::MoveLeft { select: false }]);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Right => {
                self.run_edit_commands(&[EditCommand::MoveRight { select: false }]);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::SearchHistory => {
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
        if self.history_cursor_on_excluded {
            self.history_cursor_on_excluded = false;
        }
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.history_cursor = HistoryCursor::new(
                self.get_history_navigation_based_on_line_buffer(),
                self.get_history_session_id(),
            );

            if self.history_excluded_item.is_some() {
                self.history_cursor_on_excluded = true;
            }
        }

        if !self.history_cursor_on_excluded {
            self.history_cursor
                .back(self.history.as_ref())
                .expect("todo: error handling");
        }
        self.update_buffer_from_history();
        self.editor.move_to_start(false);
        self.editor
            .update_undo_state(UndoBehavior::HistoryNavigation);
        self.editor.move_to_line_end(false);
        self.editor
            .update_undo_state(UndoBehavior::HistoryNavigation);
    }

    fn next_history(&mut self) {
        if self.input_mode != InputMode::HistoryTraversal {
            self.input_mode = InputMode::HistoryTraversal;
            self.history_cursor = HistoryCursor::new(
                self.get_history_navigation_based_on_line_buffer(),
                self.get_history_session_id(),
            );
        }

        if self.history_cursor_on_excluded {
            self.history_cursor_on_excluded = false;
        } else {
            let cursor_was_on_item = self.history_cursor.string_at_cursor().is_some();
            self.history_cursor
                .forward(self.history.as_ref())
                .expect("todo: error handling");

            if cursor_was_on_item
                && self.history_cursor.string_at_cursor().is_none()
                && self.history_excluded_item.is_some()
            {
                self.history_cursor_on_excluded = true;
            }
        }

        if self.history_cursor.string_at_cursor().is_none() && !self.history_cursor_on_excluded {
            self.input_mode = InputMode::Regular;
        }
        self.update_buffer_from_history();
        self.editor.move_to_end(false);
        self.editor
            .update_undo_state(UndoBehavior::HistoryNavigation)
    }

    /// Enable the search and navigation through the history from the line buffer prompt
    ///
    /// Enables either prefix search with output in the line buffer or simple traversal
    fn get_history_navigation_based_on_line_buffer(&self) -> HistoryNavigationQuery {
        if self.editor.is_empty() || !self.editor.is_cursor_at_buffer_end() {
            // Perform bash-style basic up/down entry walking
            HistoryNavigationQuery::Normal(
                // Hack: Tight coupling point to be able to restore previously typed input
                self.editor.line_buffer().clone(),
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
        self.history_cursor = HistoryCursor::new(
            HistoryNavigationQuery::SubstringSearch("".to_string()),
            self.get_history_session_id(),
        );
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
                        self.history_cursor = HistoryCursor::new(
                            HistoryNavigationQuery::SubstringSearch(substring),
                            self.get_history_session_id(),
                        );
                    } else {
                        self.history_cursor = HistoryCursor::new(
                            HistoryNavigationQuery::SubstringSearch(String::from(*c)),
                            self.get_history_session_id(),
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
                            self.get_history_session_id(),
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
            _ if self.history_cursor_on_excluded => self.editor.set_buffer(
                self.history_excluded_item
                    .as_ref()
                    .unwrap()
                    .command_line
                    .clone(),
                UndoBehavior::HistoryNavigation,
            ),
            HistoryNavigationQuery::Normal(original) => {
                if let Some(buffer_to_paint) = self.history_cursor.string_at_cursor() {
                    self.editor
                        .set_buffer(buffer_to_paint, UndoBehavior::HistoryNavigation);
                } else {
                    // Hack
                    self.editor
                        .set_line_buffer(original, UndoBehavior::HistoryNavigation);
                }
            }
            HistoryNavigationQuery::PrefixSearch(prefix) => {
                if let Some(prefix_result) = self.history_cursor.string_at_cursor() {
                    self.editor
                        .set_buffer(prefix_result, UndoBehavior::HistoryNavigation);
                } else {
                    self.editor
                        .set_buffer(prefix, UndoBehavior::HistoryNavigation);
                }
            }
            HistoryNavigationQuery::SubstringSearch(_) => todo!(),
        }
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        if self.input_mode == InputMode::HistoryTraversal {
            if matches!(
                self.history_cursor.get_navigation(),
                HistoryNavigationQuery::Normal(_)
            ) {
                if let Some(string) = self.history_cursor.string_at_cursor() {
                    self.editor
                        .set_buffer(string, UndoBehavior::HistoryNavigation);
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
        let parsed_prefix = parsed.prefix.unwrap_or_default().to_string();
        let parsed_marker = parsed.marker.unwrap_or_default().to_string();

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
                        filter: SearchFilter::anything(self.get_history_session_id()),
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
                        filter: SearchFilter::anything(self.get_history_session_id()),
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
                ParseAction::BackwardPrefixSearch => {
                    let history_search_by_session = self
                        .history
                        .search(SearchQuery::last_with_prefix_and_cwd(
                            parsed.prefix.unwrap().to_string(),
                            self.cwd.clone().unwrap_or_else(|| {
                                std::env::current_dir()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            }),
                            self.get_history_session_id(),
                        ))
                        .unwrap_or_else(|_| Vec::new())
                        .get(index.saturating_sub(1))
                        .map(|history| {
                            (
                                parsed.remainder.len(),
                                parsed_prefix.len() + parsed_marker.len(),
                                history.command_line.clone(),
                            )
                        });
                    // If we don't find any history searching by session id, then let's
                    // search everything, otherwise use the result from the session search
                    if history_search_by_session.is_none() {
                        self.history
                            .search(SearchQuery::last_with_prefix(
                                parsed_prefix.clone(),
                                self.get_history_session_id(),
                            ))
                            .unwrap_or_else(|_| Vec::new())
                            .get(index.saturating_sub(1))
                            .map(|history| {
                                (
                                    parsed.remainder.len(),
                                    parsed_prefix.len() + parsed_marker.len(),
                                    history.command_line.clone(),
                                )
                            })
                    } else {
                        history_search_by_session
                    }
                }
                ParseAction::ForwardSearch => self
                    .history
                    .search(SearchQuery {
                        direction: SearchDirection::Forward,
                        start_time: None,
                        end_time: None,
                        start_id: None,
                        end_id: None,
                        limit: Some((index + 1) as i64), // fetch the oldest n entries
                        filter: SearchFilter::anything(self.get_history_session_id()),
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
                    .search(SearchQuery::last_with_search(SearchFilter::anything(
                        self.get_history_session_id(),
                    )))
                    .unwrap_or_else(|_| Vec::new())
                    .first()
                    //BUGBUG: This returns the wrong results with paths with spaces in them
                    .and_then(|history| history.command_line.split_whitespace().next_back())
                    .map(|token| (parsed.remainder.len(), indicator.len(), token.to_string())),
            });

        if let Some((start, size, history)) = history_result {
            let edits = vec![
                EditCommand::MoveToPosition {
                    position: start,
                    select: false,
                },
                EditCommand::ReplaceChars(size, history),
            ];

            Some(ReedlineEvent::Edit(edits))
        } else {
            None
        }
    }

    fn open_editor(&mut self) -> Result<()> {
        match &mut self.buffer_editor {
            Some(BufferEditor {
                ref mut command,
                ref temp_file,
            }) => {
                {
                    let mut file = File::create(temp_file)?;
                    write!(file, "{}", self.editor.get_buffer())?;
                }
                {
                    let mut child = command.spawn()?;
                    child.wait()?;
                }

                let res = std::fs::read_to_string(temp_file)?;
                let res = res.trim_end().to_string();

                self.editor.set_buffer(res, UndoBehavior::CreateUndoPoint);

                Ok(())
            }
            _ => Ok(()),
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

            self.painter.repaint_buffer(
                prompt,
                &lines,
                self.prompt_edit_mode(),
                None,
                self.use_ansi_coloring,
                &self.cursor_shapes,
            )?;
        }

        Ok(())
    }

    /// Triggers a full repaint including the prompt parts
    ///
    /// Includes the highlighting and hinting calls.
    fn buffer_paint(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let cursor_position_in_buffer = self.editor.insertion_point();
        let buffer_to_paint = self.editor.get_buffer();

        let mut styled_text = self
            .highlighter
            .highlight(buffer_to_paint, cursor_position_in_buffer);
        if let Some((from, to)) = self.editor.get_selection() {
            styled_text.style_range(from, to, self.visual_selection_style);
        }

        let (before_cursor, after_cursor) = styled_text.render_around_insertion_point(
            cursor_position_in_buffer,
            prompt,
            self.use_ansi_coloring,
        );

        let hint: String = if self.hints_active() {
            self.hinter.as_mut().map_or_else(String::new, |hinter| {
                hinter.handle(
                    buffer_to_paint,
                    cursor_position_in_buffer,
                    self.history.as_ref(),
                    self.use_ansi_coloring,
                    &self.cwd.clone().unwrap_or_else(|| {
                        std::env::current_dir()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    }),
                )
            })
        } else {
            String::new()
        };

        // Needs to add return carriage to newlines because when not in raw mode
        // some OS don't fully return the carriage

        let mut lines = PromptLines::new(
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
                lines.prompt_indicator = menu.indicator().to_owned().into();
                // If the menu requires the cursor position, update it (ide menu)
                let cursor_pos = lines.cursor_pos(self.painter.screen_width());
                menu.set_cursor_pos(cursor_pos);

                menu.update_working_details(
                    &mut self.editor,
                    self.completer.as_mut(),
                    self.history.as_ref(),
                    &self.painter,
                );
            }
        }

        let menu = self.menus.iter().find(|menu| menu.is_active());

        self.painter.repaint_buffer(
            prompt,
            &lines,
            self.prompt_edit_mode(),
            menu,
            self.use_ansi_coloring,
            &self.cursor_shapes,
        )
    }

    /// Adds an external printer
    ///
    /// ## Required feature:
    /// `external_printer`
    #[cfg(feature = "external_printer")]
    pub fn with_external_printer(mut self, printer: ExternalPrinter<String>) -> Self {
        self.external_printer = Some(printer);
        self
    }

    #[cfg(feature = "external_printer")]
    fn external_messages(external_printer: &ExternalPrinter<String>) -> Result<Vec<String>> {
        let mut messages = Vec::new();
        loop {
            let result = external_printer.receiver().try_recv();
            match result {
                Ok(line) => {
                    let lines = line.lines().map(String::from).collect::<Vec<_>>();
                    messages.extend(lines);
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    return Err(Error::new(
                        ErrorKind::NotConnected,
                        TryRecvError::Disconnected,
                    ));
                }
            }
        }
        Ok(messages)
    }

    fn submit_buffer(&mut self, prompt: &dyn Prompt) -> io::Result<EventStatus> {
        let buffer = self.editor.get_buffer().to_string();
        self.hide_hints = true;
        // Additional repaint to show the content without hints etc.
        if let Some(transient_prompt) = self.transient_prompt.take() {
            self.repaint(transient_prompt.as_ref())?;
            self.transient_prompt = Some(transient_prompt);
        } else {
            self.repaint(prompt)?;
        }
        if !buffer.is_empty() {
            let mut entry = HistoryItem::from_command_line(&buffer);
            entry.session_id = self.get_history_session_id();

            if self
                .history_exclusion_prefix
                .as_ref()
                .map(|prefix| buffer.starts_with(prefix))
                .unwrap_or(false)
            {
                entry.id = Some(Self::FILTERED_ITEM_ID);
                self.history_last_run_id = entry.id;
                self.history_excluded_item = Some(entry);
            } else {
                entry = self.history.save(entry).expect("todo: error handling");
                self.history_last_run_id = entry.id;
                self.history_excluded_item = None;
            }
        }
        self.run_edit_commands(&[EditCommand::Clear]);
        self.editor.reset_undo_stack();

        Ok(EventStatus::Exits(Signal::Success(buffer)))
    }
}

#[test]
fn thread_safe() {
    fn f<S: Send>(_: S) {}
    f(Reedline::create());
}
