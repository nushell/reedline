use std::{collections::HashMap, ops::ControlFlow, path::PathBuf};

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
    std::io::{Error, ErrorKind},
    std::sync::mpsc::TryRecvError,
};
use {
    crate::{
        completion::{Completer, CompletionOrigin, CompletionStatus, DefaultCompleter},
        core_editor::Editor,
        edit_mode::{EditMode, Emacs},
        enums::{EventStatus, ReedlineEvent},
        highlighter::SimpleMatchHighlighter,
        hinter::Hinter,
        history::{
            FileBackedHistory, History, HistoryCursor, HistoryItem, HistoryItemId,
            HistoryNavigationQuery, HistorySessionId, SearchDirection, SearchQuery,
        },
        painting::{Painter, PainterSuspendedState, PromptLines, RenderSnapshot, W},
        prompt::{PromptEditMode, PromptHistorySearchStatus},
        result::{ReedlineError, ReedlineErrorVariants},
        terminal_extensions::{
            bracketed_paste::BracketedPasteGuard,
            kitty::KittyProtocolGuard,
            semantic_prompt::{Osc133ClickEventsMarkers, SemanticPromptMarkers},
        },
        utils::text_manipulation,
        AbbrExpandContext, AutoPairAction, AutoPairContext, AutoPairs, Direction, EditCommand,
        ExampleHighlighter, Highlighter, LineBuffer, Menu, MenuEvent, MouseButton, Prompt,
        PromptHistorySearch, ReedlineMenu, Signal, UndoBehavior, ValidationResult, Validator,
    },
    crossterm::{
        cursor::{SetCursorStyle, Show},
        event,
        event::{Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, QueueableCommand,
    },
    std::{
        fs::File,
        io,
        io::Result,
        io::Write,
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
        time::SystemTime,
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

/// Default maximum time Reedline will block on input before yielding control
/// for features that require periodic processing (e.g., external printer,
/// idle callback).
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

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

/// Configuration for mouse click-to-cursor support.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MouseClickMode {
    /// Disable mouse click handling.
    #[default]
    Disabled,
    /// Enable mouse click handling without emitting OSC 133 markers.
    Enabled,
    /// Enable mouse click handling and emit OSC 133 markers with `click_events=1`.
    EnabledWithOsc133,
}

impl MouseClickMode {
    fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled | Self::EnabledWithOsc133)
    }
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
    /// Last failed `history.save`, until [`Reedline::take_history_save_error`].
    history_save_error: Option<ReedlineError>,
    input_mode: InputMode,

    // State of the painter after a `ReedlineEvent::ExecuteHostCommand` was requested, used after
    // execution to decide if we can re-use the previous prompt or paint a new one.
    suspended_state: Option<PainterSuspendedState>,
    last_render_snapshot: Option<RenderSnapshot>,

    // Validator
    validator: Option<Box<dyn Validator>>,

    // Stdout
    painter: Painter,

    transient_prompt: Option<Box<dyn Prompt>>,

    // Edit Mode: Vi, Emacs
    edit_mode: Box<dyn EditMode>,

    // Provides the tab completions
    completer: Box<dyn Completer + Send>,
    quick_completions: bool,
    partial_completions: bool,
    persistent_menus: bool,
    // Completions owed to a menu activation the completer could not answer in time
    deferred_menu_completion: Option<DeferredMenuCompletion>,

    // Highlight the edit buffer
    highlighter: Box<dyn Highlighter>,

    // Style used for visual selection
    visual_selection_style: Style,
    /// A distinct style for the cell under the cursor inside a selection;
    /// `None` leaves the cell on `visual_selection_style`.
    visual_selection_cursor_style: Option<Style>,

    // Showcase hints based on various strategies (history, language-completion, spellcheck, etc)
    hinter: Option<Box<dyn Hinter>>,
    hide_hints: bool,

    // Use ansi coloring or not
    use_ansi_coloring: bool,

    // Automatically insert and manage configured character pairs.
    auto_pairs: Option<AutoPairs>,

    // Whether to enable mouse click-to-cursor functionality
    mouse_click_mode: MouseClickMode,

    // Current working directory as defined by the application. If set, it will
    // override the actual working directory of the process.
    cwd: Option<String>,

    // Engine Menus
    menus: Vec<ReedlineMenu>,

    abbreviations: HashMap<String, String>,

    // Text editor used to open the line buffer for editing
    buffer_editor: Option<BufferEditor>,

    // Use different cursors depending on the current edit mode
    cursor_shapes: Option<CursorConfig>,

    // Manage bracketed paste mode
    bracketed_paste: BracketedPasteGuard,

    // Manage optional kitty protocol
    kitty_protocol: KittyProtocolGuard,

    // Whether lines should be accepted immediately
    immediately_accept: bool,

    // External break signal: when set to `true`, `read_line()` will return
    // `Signal::ExternalBreak` with the current buffer contents.
    break_signal: Option<Arc<AtomicBool>>,

    // External repaint signal: when triggered, the prompt is re-evaluated
    // and repainted in place while `read_line()` is running.
    repaint_signal: Option<RepaintSignal>,

    // Maximum time to block on input before yielding control for features that
    // require periodic processing (external printer, idle callback).
    // Only used when external_printer or idle_callback is configured.
    poll_interval: Duration,

    #[cfg(feature = "external_printer")]
    external_printer: Option<ExternalPrinter<String>>,

    // Callback function that is called periodically while waiting for input.
    // Useful for processing external events (e.g., GUI updates) during idle time.
    idle_callback: Option<Box<dyn FnMut() + Send>>,
}

struct BufferEditor {
    command: Command,
    temp_file: PathBuf,
}

/// The completions the [`Menu`](ReedlineEvent::Menu) event could not decide, because the
/// completer had not answered yet.
///
/// Activating a menu immediately inspects its values twice: quick completions accept a
/// lone suggestion, and partial completions splice in the prefix the suggestions share.
/// A completer computing in the background can answer neither yet.
///
/// The snapshot pins the editor state as of the keystroke this was armed on: a result
/// landing after the user has typed on is discarded rather than rewriting the line
/// underneath them.
struct DeferredMenuCompletion {
    menu: String,
    /// The line this was armed on, stamped as a completer stamps its own results.
    origin: CompletionOrigin,
}

impl DeferredMenuCompletion {
    fn new(menu: &ReedlineMenu, editor: &Editor) -> Self {
        Self {
            menu: menu.name().to_string(),
            origin: CompletionOrigin::new(editor.get_buffer(), editor.insertion_point()),
        }
    }

    /// Whether the line is still exactly as it was when this was armed, on the same menu.
    /// Anything else means the user moved on and the decision is void.
    fn still_applies(&self, menu: &ReedlineMenu, editor: &Editor) -> bool {
        self.menu == menu.name()
            && self
                .origin
                .matches(editor.get_buffer(), editor.insertion_point())
    }
}

/// Call [`request_repaint`](RepaintSignal::request_repaint) once
/// new prompt data is ready! The next iteration of the input loop re-evaluates
/// the [`Prompt`] and redraws it without interrupting the current line edit.
#[derive(Clone, Debug, Default)]
pub struct RepaintSignal {
    flag: Arc<AtomicBool>,
}

impl RepaintSignal {
    /// Request that the prompt is re-evaluated and repainted in place.
    pub fn request_repaint(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    /// Consume a pending repaint request.
    fn take(&self) -> bool {
        self.flag.swap(false, Ordering::Relaxed)
    }
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

/// Mark the painter's cached prompt anchor stale around a menu running its completer,
/// which may have left the cached row pointing at content that has scrolled. See
/// [`ReedlineMenu::queries_host_completer`] for why only some completers count, and
/// #1130 for the bug.
///
/// Also skipped for a menu the same keystroke deactivated, whose queued event nothing
/// goes on to consume.
///
/// Call this from every event that reaches the completer, which is not the same set as
/// the events that change the menu's selection: `MenuNext` splices a partial completion
/// and queries, while `MenuPrevious` only moves and does not.
fn invalidate_anchor_if_host_completer_runs(menu: &ReedlineMenu, painter: &mut Painter) {
    if menu.is_active() && menu.queries_host_completer() {
        painter.invalidate_prompt_start_row();
    }
}

impl Reedline {
    const FILTERED_ITEM_ID: HistoryItemId = HistoryItemId(i64::MAX);

    /// Create a new [`Reedline`] engine with a local [`History`] that is not synchronized to a file.
    #[must_use]
    pub fn create() -> Self {
        let history = Box::<FileBackedHistory>::default();
        #[cfg(not(test))]
        let painter = Painter::new(W::terminal());
        #[cfg(test)]
        let painter = Painter::new(W::sink());
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
            history_save_error: None,
            input_mode: InputMode::Regular,
            suspended_state: None,
            last_render_snapshot: None,
            painter,
            transient_prompt: None,
            edit_mode,
            completer,
            quick_completions: false,
            partial_completions: false,
            persistent_menus: false,
            deferred_menu_completion: None,
            highlighter: buffer_highlighter,
            visual_selection_style,
            visual_selection_cursor_style: None,
            hinter,
            hide_hints: false,
            validator,
            use_ansi_coloring: true,
            auto_pairs: None,
            mouse_click_mode: MouseClickMode::default(),
            cwd: None,
            menus: Vec::new(),
            abbreviations: HashMap::new(),
            buffer_editor: None,
            cursor_shapes: None,
            bracketed_paste: BracketedPasteGuard::default(),
            kitty_protocol: KittyProtocolGuard::default(),
            immediately_accept: false,
            break_signal: None,
            repaint_signal: None,
            poll_interval: DEFAULT_POLL_INTERVAL,
            #[cfg(feature = "external_printer")]
            external_printer: None,
            idle_callback: None,
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
    pub fn with_completer(mut self, completer: Box<dyn Completer + Send>) -> Self {
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

    /// Control whether the cursor crosses line boundaries on left/right motions
    /// in a block caret (vi normal/visual mode). When `true` (the default), `l`
    /// at the end of a line moves to the next line's first character and `h` at
    /// column 0 to the previous line's last; when `false`, both stop at the line
    /// edge (vim's default `h`/`l`). Has no effect on emacs or vi insert mode,
    /// whose bar caret always moves freely across lines.
    ///
    /// Scope: this steers where the **caret rests** on `h`/`l`, not how far an
    /// operator reaches. Operator motions (`d`/`c`/`y`) delete the literal
    /// grapheme span regardless of this flag, so e.g. `dl` always deletes the
    /// char under the caret and never the line break.
    #[must_use]
    pub fn with_cross_line_cursor(mut self, cross_line_cursor: bool) -> Self {
        self.editor.set_cross_line_cursor(cross_line_cursor);
        self
    }

    /// Turn on partial completions. These completions will fill the buffer with the
    /// smallest common string from all the options
    #[must_use]
    pub fn with_partial_completions(mut self, partial_completions: bool) -> Self {
        self.partial_completions = partial_completions;
        self
    }

    /// Make active menus persist while the line is edited: erasing characters
    /// or emptying the line refilters the menu instead of dismissing it.
    ///
    /// When disabled (the default), an active menu is deactivated by a backspace
    /// when quick completions are on, and by any edit that leaves the line
    /// buffer empty. A persistent menu still closes on Esc, Ctrl-C, or when a
    /// value is accepted.
    #[must_use]
    pub fn with_persistent_menus(mut self, persistent_menus: bool) -> Self {
        self.persistent_menus = persistent_menus;
        self
    }

    /// A builder which enables or disables the use of ansi coloring in the prompt
    /// and in the command line syntax highlighting.
    #[must_use]
    pub fn with_ansi_colors(mut self, use_ansi_coloring: bool) -> Self {
        self.use_ansi_coloring = use_ansi_coloring;
        self
    }

    /// A builder that configures automatic pair insertion for the Reedline engine.
    ///
    /// By default, auto-pairing applies at every position for the configured pairs.
    /// To suppress it in certain syntactic positions (e.g. string literals, or in the
    /// middle of a word), override [`Highlighter::should_auto_pair`].
    ///
    /// For `InsertChar`, the closer is looked up before the opener. A character that
    /// is registered as both a closer of one pair and an opener of another therefore
    /// resolves based on whether that closer currently sits at the cursor, not on the
    /// order the pairs were passed to [`AutoPairs::new`].
    ///
    /// This builder does not touch the terminal's bracketed paste setting.
    /// If bracketed paste is enabled (see [`Self::use_bracketed_paste`]), a paste is
    /// delivered as [`EditCommand::InsertString`](crate::EditCommand::InsertString),
    /// which never goes through auto-pairing. If bracketed paste is disabled, pasted
    /// characters arrive the same way as typed ones and are auto-paired like typing:
    /// pasting text that has an opener without its closer (e.g. `foo(bar`) inserts a
    /// closing character that was never in the clipboard content. For this reason it
    /// is recommended to combine `with_auto_pairs` with
    /// `use_bracketed_paste(cfg!(not(target_os = "windows")))`. On Windows,
    /// stock crossterm reads console input through
    /// the Win32 console API and has no ANSI input parser, so there is no
    /// `Event::Paste` path to enable there (see crossterm-rs/crossterm#737).
    ///
    /// Auto-pairing applies to edits in the regular line buffer. Reverse-history
    /// search edits a separate search query and are not passed through the
    /// auto-pairing machinery.
    #[must_use]
    pub fn with_auto_pairs(mut self, auto_pairs: AutoPairs) -> Self {
        self.auto_pairs = Some(auto_pairs);
        self
    }

    /// Disable automatic pair insertion.
    #[must_use]
    pub fn disable_auto_pairs(mut self) -> Self {
        self.auto_pairs = None;
        self
    }

    /// Configure mouse click-to-cursor support.
    ///
    /// Use [`MouseClickMode::Enabled`] to handle click events when your host shell
    /// emits OSC 133 markers. Use [`MouseClickMode::EnabledWithOsc133`] to have
    /// Reedline emit OSC 133 markers with `click_events=1` so supporting terminals
    /// can send click events.
    /// See: <https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers>
    #[must_use]
    pub fn with_mouse_click(mut self, mode: MouseClickMode) -> Self {
        self.mouse_click_mode = mode;
        if matches!(mode, MouseClickMode::EnabledWithOsc133) {
            self.painter
                .set_semantic_markers(Some(Osc133ClickEventsMarkers::boxed()));
        }
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

    /// A builder that gives the cell under the cursor its own style inside a
    /// visual selection, the way helix styles its primary cursor distinctly
    /// from the selection around it. Left unset, the cell keeps the plain
    /// selection style, which can paint a flat selection (e.g. reverse video)
    /// over the terminal cursor and hide it.
    #[must_use]
    pub fn with_visual_selection_cursor_style(mut self, style: Style) -> Self {
        self.visual_selection_cursor_style = Some(style);
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

    /// A builder that configures semantic prompt markers for terminal integration.
    ///
    /// This enables semantic prompt support for terminals that support it, such as Ghostty.
    /// Use `Osc133Markers::boxed()` for standard terminal support or `Osc633Markers::boxed()`
    /// for VS Code integrated terminal support.
    #[must_use]
    pub fn with_semantic_markers(
        mut self,
        markers: Option<Box<dyn SemanticPromptMarkers>>,
    ) -> Self {
        self.painter.set_semantic_markers(markers);
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

    /// A builder that adds abbreviations to the Reedline engine
    ///
    /// Overwrites any existing abbreviations with the same key.
    ///
    /// Note, by default abbreviations are expanded everywhere. To suppress expansion in certain
    /// syntactic positions (e.g. string literals), override [`Highlighter::should_expand_abbr`].
    pub fn with_abbreviations(mut self, abbreviations: HashMap<String, String>) -> Self {
        self.abbreviations.extend(abbreviations);
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

    /// A builder that configures whether reedline should immediately accept the input.
    pub fn with_immediately_accept(mut self, immediately_accept: bool) -> Self {
        self.immediately_accept = immediately_accept;
        self
    }

    /// A builder that configures an external break signal.
    ///
    /// When the [`AtomicBool`] is set to `true` by an external thread,
    /// [`Reedline::read_line()`] will return [`Signal::ExternalBreak`] with the
    /// current buffer contents. The flag is automatically reset to `false`
    /// after being consumed.
    pub fn with_break_signal(mut self, signal: Arc<AtomicBool>) -> Self {
        self.break_signal = Some(signal);
        self
    }

    /// Get a [`RepaintSignal`] handle that can trigger an in-place repaint of
    /// the prompt from another thread while [`Reedline::read_line()`] is
    /// running, avoiding interfering with current line edit.
    pub fn repaint_signal(&mut self) -> RepaintSignal {
        // The handle is created lazily on the first call; subsequent calls return clones
        self.repaint_signal
            .get_or_insert_with(RepaintSignal::default)
            .clone()
    }

    /// Returns the corresponding expected prompt style for the given edit mode
    pub fn prompt_edit_mode(&self) -> PromptEditMode {
        self.edit_mode.edit_mode()
    }

    /// Output the complete [`History`] chronologically with numbering to the terminal
    pub fn print_history(&mut self) -> Result<()> {
        let history: Vec<_> = self
            .history
            .search(SearchQuery::everything(SearchDirection::Forward, None))?;

        for (i, entry) in history.iter().enumerate() {
            self.print_line(&format!("{}\t{}", i, entry.command_line))?;
        }
        Ok(())
    }

    /// Output the complete [`History`] for this session, chronologically with numbering to the terminal
    pub fn print_history_session(&mut self) -> Result<()> {
        let history: Vec<_> = self.history.search(SearchQuery::everything(
            SearchDirection::Forward,
            self.get_history_session_id(),
        ))?;

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
                self.history_excluded_item = self.history_excluded_item.take().map(f);
                Ok(())
            }
            Some(r) => self.history.update(*r, f),
            None => Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                "No command run",
            ))),
        }
    }

    /// Take the error of the last failed history save, if any.
    ///
    /// [`read_line`](Self::read_line) still returns the line when the [`History`]
    /// refuses to store it; the entry is then treated like an excluded one.
    /// Cleared on read, set at most once per `read_line`.
    pub fn take_history_save_error(&mut self) -> Option<ReedlineError> {
        self.history_save_error.take()
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

    /// Consume a pending external repaint request, returning whether one was
    /// pending. Any number of requests since the last check collapse into one.
    fn take_repaint_request(&self) -> bool {
        self.repaint_signal
            .as_ref()
            .is_some_and(RepaintSignal::take)
    }

    /// Whether the input loop must poll with a timeout instead of blocking
    /// indefinitely, so external triggers (break signal, repaint signal,
    /// external printer, idle callback) are noticed while waiting for input.
    fn input_needs_polling(&self) -> bool {
        #[allow(unused_mut)] // Dependent on feature flags
        let mut poll = self.break_signal.is_some()
            || self
                .repaint_signal
                .as_ref()
                .is_some_and(|sig| Arc::strong_count(&sig.flag) > 1);

        #[cfg(feature = "external_printer")]
        {
            poll |= self.external_printer.is_some();
        }

        poll |= self.idle_callback.is_some();

        poll
    }

    /// Helper implementing the logic for [`Reedline::read_line()`] to be wrapped
    /// in a `raw_mode` context.
    fn read_line_helper(&mut self, prompt: &dyn Prompt) -> Result<Signal> {
        self.painter
            .initialize_prompt_position(self.suspended_state.as_ref())?;
        if self.suspended_state.is_some() {
            // Last editor was suspended (ExecuteHostCommand or ExternalBreak),
            // we are resuming operation now.
            self.suspended_state = None;
        }
        self.hide_hints = false;

        // Repaint requests raised while no read_line was active are stale:
        // the fresh prompt painted below already reflects the latest state.
        self.take_repaint_request();

        self.repaint(prompt)?;

        loop {
            // Call idle callback if set (for processing external events like GUI updates)
            if let Some(ref mut callback) = self.idle_callback {
                callback();
                // The callback owns stdout while it runs and may have
                // written or moved the cursor. Re-verify the anchor on
                // the next paint.
                self.painter.invalidate_prompt_start_row();
            }

            if let Some(ref signal) = self.break_signal {
                if signal.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    let buffer = self.editor.get_buffer().to_string();
                    self.input_mode = InputMode::Regular;
                    self.last_render_snapshot = None;
                    self.suspended_state = Some(self.painter.state_before_suspension());
                    self.editor.reset_undo_stack();
                    return Ok(Signal::ExternalBreak(buffer));
                }
            }

            if self.take_repaint_request() {
                self.repaint(prompt)?;
            }

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

            // Determine if we need to poll (non-blocking) or can block on input.
            // We need polling if external_printer or idle_callback is configured,
            // using the shared poll_interval for the timeout.
            let status = self.completer.poll_completion();
            // Anything BUT idle means work is in flight. We need to keep polling.
            let completer_pending = status != CompletionStatus::Idle;

            if status == CompletionStatus::Ready {
                self.settle_completions(prompt)?;
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

            if !self.immediately_accept {
                if self.input_needs_polling() || completer_pending {
                    if event::poll(self.poll_interval)? {
                        events.push(crossterm::event::read()?);
                    }
                } else {
                    // Block until we receive an event
                    events.push(crossterm::event::read()?);
                }

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
            }

            // Process the batch unconditionally: in `immediately_accept` mode
            // `events` stays empty, but `process_input_batch` still pushes the
            // synthetic `Submit` and returns the buffer. Gating this call behind
            // `!immediately_accept` would spin the loop forever.
            if let ControlFlow::Break(signal) = self.process_input_batch(prompt, events)? {
                return Ok(signal);
            }
        }
    }

    /// Fold a finished completion request into the active menu, and honor the completions
    /// owed from when the request was made.
    ///
    /// Called when the completer reports [`CompletionStatus::Ready`]. Kept out of the
    /// input loop so it is reachable from tests, which cannot drive the loop itself.
    fn settle_completions(&mut self, prompt: &dyn Prompt) -> Result<()> {
        let Some(menu_index) = self.menus.iter().position(|menu| menu.is_active()) else {
            // No menu to answer to, so nothing can still be owed.
            self.deferred_menu_completion = None;
            return Ok(());
        };

        // The request that just finished was host code, which may have scrolled the
        // terminal while it ran in the background — after the keystroke that
        // dispatched it had already re-verified the anchor. `update_values` below
        // can also dispatch another request for a line that moved on. Either way
        // the repaint at the end of this settle must not trust the cached row.
        invalidate_anchor_if_host_completer_runs(&self.menus[menu_index], &mut self.painter);

        let menu = &mut self.menus[menu_index];
        // The request this menu was waiting on finished, so repopulate it.
        menu.update_values(
            &mut self.editor,
            self.completer.as_mut(),
            self.history.as_ref(),
        );
        let still_provisional = menu.results_are_provisional();

        let owed = self
            .deferred_menu_completion
            .as_ref()
            .is_some_and(|deferred| deferred.still_applies(menu, &self.editor));

        // Values were just refreshed above, so the menu must not re-fetch them.
        let accept_lone_value =
            owed && !still_provisional && self.decide_menu_completion(menu_index, true);

        // Spent once a final answer had its say, so it cannot act on a later one.
        // Provisional results decided nothing, so the arm outlives them.
        if !still_provisional {
            self.deferred_menu_completion = None;
        }

        if accept_lone_value {
            // With a menu active this replaces in the buffer and deactivates, rather than
            // submitting the line.
            self.handle_editor_event(prompt, ReedlineEvent::Enter)?;
        }

        // One paint for every outcome, since painting the menu and then the accepted or
        // extended line would flicker on each completion.
        self.repaint(prompt)
    }

    /// The completion an opening menu applies to the line: a lone suggestion is accepted
    /// outright, otherwise the prefix the suggestions share is spliced in. Returns whether
    /// the caller should accept that lone suggestion.
    ///
    /// Both the [`Menu`](ReedlineEvent::Menu) event and the deferred replay in
    /// [`settle_completions`](Self::settle_completions) decide this, and they have to
    /// decide it identically. `values_updated` says whether the caller already refreshed
    /// the menu's values, so they are not fetched twice.
    fn decide_menu_completion(&mut self, menu_index: usize, values_updated: bool) -> bool {
        let menu = &mut self.menus[menu_index];

        let accept_lone_value = if self.quick_completions && menu.can_quick_complete() {
            if !values_updated {
                menu.update_values(
                    &mut self.editor,
                    self.completer.as_mut(),
                    self.history.as_ref(),
                );
            }
            // Accepting a lone *stale* value is refused downstream, since its span
            // belongs to another line, so the menu would close over a completion that
            // never happened.
            menu.get_values().len() == 1 && !menu.results_are_provisional()
        } else {
            false
        };

        if !accept_lone_value && self.partial_completions {
            menu.can_partially_complete(
                values_updated || self.quick_completions,
                &mut self.editor,
                self.completer.as_mut(),
                self.history.as_ref(),
            );
        }

        accept_lone_value
    }

    fn process_input_batch(
        &mut self,
        prompt: &dyn Prompt,
        events: Vec<Event>,
    ) -> Result<ControlFlow<Signal>> {
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
                            reedline_events.push(ReedlineEvent::Edit(std::mem::take(&mut edits)));
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
        if self.immediately_accept {
            reedline_events.push(ReedlineEvent::Submit);
        }

        // The mode machine has parsed this batch, so the rest policy it
        // declares is now final. Relay it to the editor before running the
        // emitted commands so a command a mode transition issued (e.g. the
        // Esc→normal grapheme step-back) resolves under the new policy. This
        // does not commit the cursor — the commands settle it, and the
        // pre-paint `set_edit_mode` below still clamps no-command switches.
        self.editor.sync_edit_mode(self.edit_mode.edit_mode());

        // Handle reedline events.
        let mut need_repaint = false;
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
                    return Ok(ControlFlow::Break(signal));
                }
                EventStatus::Handled => {
                    need_repaint = true;
                }
                EventStatus::Inapplicable => {
                    // Nothing changed, no need to repaint
                }
            }
        }
        // A command-less mode transition adopts a new rest policy via
        // `sync_edit_mode` but emits nothing to commit the cursor. Force the
        // settle (and a repaint) so it doesn't stay unsettled until the next
        // command.
        if self.editor.policy_unsettled() {
            need_repaint = true;
        }
        if need_repaint {
            // Sync the editor's edit mode before painting so the cursor is
            // normalized under the current rest policy. A mode change that
            // bypasses the command path (e.g. Esc → Vi normal) otherwise
            // wouldn't clamp until the next command, painting the cursor past
            // the last grapheme for a frame.
            let mode = self.edit_mode.edit_mode();
            self.editor.set_edit_mode(mode);
            self.repaint(prompt)?;
        }
        Ok(ControlFlow::Continue(()))
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
                // No candidate applied, so nothing changed: report that, which
                // also lets an enclosing `UntilFound` keep trying.
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::CtrlD => {
                if self.editor.is_empty() {
                    self.input_mode = InputMode::Regular;
                    self.editor.reset_undo_stack();
                    Ok(EventStatus::Exits(Signal::CtrlD))
                } else {
                    self.run_history_commands(&[EditCommand::Delete])?;
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
                self.last_render_snapshot = None;
                self.suspended_state = Some(self.painter.state_before_suspension());
                Ok(EventStatus::Exits(Signal::HostCommand(host_command)))
            }
            ReedlineEvent::Edit(commands) => {
                self.run_history_commands(&commands)?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Mouse {
                column,
                row,
                button,
            } => {
                if button == MouseButton::Left {
                    self.handle_mouse_click(column, row)?;
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Resize(width, height) => {
                self.last_render_snapshot = None;
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                // A handled Event causes a repaint
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::PreviousHistory | ReedlineEvent::Up | ReedlineEvent::SearchHistory => {
                self.history_cursor.back(self.history.as_ref())?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory | ReedlineEvent::Down => {
                self.history_cursor.forward(self.history.as_ref())?;
                // Hacky way to ensure that we don't fall of into failed search going forward
                if self.history_cursor.string_at_cursor().is_none() {
                    self.history_cursor.back(self.history.as_ref())?;
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
            | ReedlineEvent::ToStart
            | ReedlineEvent::ToEnd
            | ReedlineEvent::Multiple(_)
            | ReedlineEvent::None
            | ReedlineEvent::HistoryHintWordComplete
            | ReedlineEvent::OpenEditor
            | ReedlineEvent::Menu(_)
            | ReedlineEvent::MenuAccept
            | ReedlineEvent::MenuNext
            | ReedlineEvent::MenuPrevious
            | ReedlineEvent::MenuUp
            | ReedlineEvent::MenuDown
            | ReedlineEvent::MenuLeft
            | ReedlineEvent::MenuRight
            | ReedlineEvent::MenuPageNext
            | ReedlineEvent::MenuPagePrevious
            | ReedlineEvent::ViChangeMode(_) => Ok(EventStatus::Inapplicable),
            ReedlineEvent::HelixChangeMode(_) => Ok(EventStatus::Inapplicable),
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
                    if let Some(index) = self.menus.iter().position(|menu| menu.name() == name) {
                        self.menus[index].menu_event(MenuEvent::Activate(self.quick_completions));
                        invalidate_anchor_if_host_completer_runs(
                            &self.menus[index],
                            &mut self.painter,
                        );

                        if self.decide_menu_completion(index, false) {
                            return self.handle_editor_event(prompt, ReedlineEvent::Enter);
                        }

                        // A final answer already had its say above, so only a
                        // provisional one leaves anything owed. With neither option set
                        // nothing queried the completer, so this reads the default
                        // `false` and the menu paints as it always has.
                        if self.menus[index].results_are_provisional() {
                            self.deferred_menu_completion = Some(DeferredMenuCompletion::new(
                                &self.menus[index],
                                &self.editor,
                            ));
                        }

                        return Ok(EventStatus::Handled);
                    }
                }
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::MenuAccept => {
                // Same accept as `Enter` over an open menu, minus the submit that
                // `Enter` falls through to when no menu is open. An empty menu has
                // nothing to splice, so that reports inapplicable too rather than
                // spending the keypress on closing it.
                match self.menus.iter_mut().find(|menu| menu.is_active()) {
                    Some(menu) if !menu.get_values().is_empty() => {
                        menu.replace_in_buffer(&mut self.editor);
                        menu.menu_event(MenuEvent::Deactivate);
                        Ok(EventStatus::Handled)
                    }
                    _ => Ok(EventStatus::Inapplicable),
                }
            }
            ReedlineEvent::MenuNext => {
                if let Some(menu) = self.menus.iter_mut().find(|menu| menu.is_active()) {
                    // The second route to the lone-value accept, so it carries the same
                    // provisional guard as `decide_menu_completion`: accepting a lone
                    // *stale* value is refused downstream, but the `Enter` would still
                    // deactivate the menu, closing it over a completion that never
                    // happened.
                    if menu.get_values().len() == 1
                        && menu.can_quick_complete()
                        && !menu.results_are_provisional()
                    {
                        self.handle_editor_event(prompt, ReedlineEvent::Enter)
                    } else {
                        if self.partial_completions {
                            menu.can_partially_complete(
                                self.quick_completions,
                                &mut self.editor,
                                self.completer.as_mut(),
                                self.history.as_ref(),
                            );
                            invalidate_anchor_if_host_completer_runs(menu, &mut self.painter);
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
            // These two spell out `active_menu()`, since that borrows all of `self` and
            // the painter has to stay reachable alongside the menu.
            ReedlineEvent::MenuPageNext => {
                match self.menus.iter_mut().find(|menu| menu.is_active()) {
                    Some(menu) => {
                        menu.menu_event(MenuEvent::NextPage);
                        invalidate_anchor_if_host_completer_runs(menu, &mut self.painter);
                        Ok(EventStatus::Handled)
                    }
                    None => Ok(EventStatus::Inapplicable),
                }
            }
            ReedlineEvent::MenuPagePrevious => {
                match self.menus.iter_mut().find(|menu| menu.is_active()) {
                    Some(menu) => {
                        menu.menu_event(MenuEvent::PreviousPage);
                        invalidate_anchor_if_host_completer_runs(menu, &mut self.painter);
                        Ok(EventStatus::Handled)
                    }
                    None => Ok(EventStatus::Inapplicable),
                }
            }
            ReedlineEvent::HistoryHintComplete => {
                let hint = self.hinter.as_mut().map(|h| h.complete_hint());
                Ok(self.accept_history_hint(hint))
            }
            ReedlineEvent::HistoryHintWordComplete => {
                let hint = self.hinter.as_mut().map(|h| h.next_hint_token());
                Ok(self.accept_history_hint(hint))
            }
            ReedlineEvent::Esc => {
                self.deactivate_menus();
                self.editor.clear_selection();
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
                if let Some(menu) = self.menus.iter_mut().find(|menu| menu.is_active()) {
                    menu.replace_in_buffer(&mut self.editor);
                    menu.menu_event(MenuEvent::Deactivate);
                    Ok(EventStatus::Handled)
                } else {
                    Ok(EventStatus::Inapplicable)
                }
            }
            ReedlineEvent::Enter => {
                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }
                if let Some(event) = self.try_expand_abbreviation_at_cursor(true) {
                    self.handle_editor_event(prompt, event)?;
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
                if let Some(event) = self.try_expand_abbreviation_at_cursor(true) {
                    self.handle_editor_event(prompt, event)?;
                }

                Ok(self.submit_buffer(prompt)?)
            }
            ReedlineEvent::SubmitOrNewline => {
                #[cfg(feature = "bashisms")]
                if let Some(event) = self.parse_bang_command() {
                    return self.handle_editor_event(prompt, event);
                }
                if let Some(event) = self.try_expand_abbreviation_at_cursor(true) {
                    self.handle_editor_event(prompt, event)?;
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
                self.last_render_snapshot = None;
                self.suspended_state = Some(self.painter.state_before_suspension());
                Ok(EventStatus::Exits(Signal::HostCommand(host_command)))
            }
            ReedlineEvent::Edit(commands) => {
                self.run_edit_commands(&commands);
                // Check if a space was just inserted and try to expand abbreviations
                if let Some(EditCommand::InsertChar(' ')) = commands.first() {
                    if let Some(event) = self.try_expand_abbreviation_at_cursor(false) {
                        return self.handle_editor_event(prompt, event);
                    }
                }
                if let Some(menu) = self.menus.iter_mut().find(|men| men.is_active()) {
                    if self.quick_completions && menu.can_quick_complete() {
                        match commands.first() {
                            Some(&EditCommand::Backspace)
                            | Some(&EditCommand::BackspaceWord)
                            | Some(&EditCommand::MoveToLineStart { select: false })
                                if !self.persistent_menus =>
                            {
                                menu.menu_event(MenuEvent::Deactivate)
                            }
                            _ => {
                                menu.menu_event(MenuEvent::Edit(self.quick_completions));
                                invalidate_anchor_if_host_completer_runs(menu, &mut self.painter);
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
                    if !self.persistent_menus && self.editor.line_buffer().get_buffer().is_empty() {
                        menu.menu_event(MenuEvent::Deactivate);
                    } else {
                        menu.menu_event(MenuEvent::Edit(self.quick_completions));
                        invalidate_anchor_if_host_completer_runs(menu, &mut self.painter);
                    }
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::OpenEditor => self.open_editor().map(|_| EventStatus::Handled),
            ReedlineEvent::Resize(width, height) => {
                self.last_render_snapshot = None;
                self.painter.handle_resize(width, height);
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Repaint => {
                // A handled Event causes a repaint
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::PreviousHistory => {
                self.previous_history()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::NextHistory => {
                self.next_history()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Up => {
                self.up_command()?;
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::Down => {
                self.down_command()?;
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
            ReedlineEvent::ToStart => {
                self.editor.move_to_start(false);
                self.editor.commit_cursor();
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::ToEnd => {
                self.editor.move_to_end(false);
                // Settle under the rest policy: `Alt+>` is bound in vi normal too,
                // where the block caret must not rest past the last grapheme.
                self.editor.commit_cursor();
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
                // No candidate applied, so nothing changed: report that, which
                // also lets an enclosing `UntilFound` keep trying.
                Ok(EventStatus::Inapplicable)
            }
            ReedlineEvent::ViChangeMode(_) => Ok(self.change_edit_mode(event)),
            ReedlineEvent::HelixChangeMode(_) => Ok(self.change_edit_mode(event)),
            ReedlineEvent::Mouse {
                column,
                row,
                button,
            } => {
                if button == MouseButton::Left {
                    self.handle_mouse_click(column, row)?;
                }
                Ok(EventStatus::Handled)
            }
            ReedlineEvent::None => Ok(EventStatus::Inapplicable),
        }
    }

    /// Route a mode-switch event to the active edit mode, then repair the cursor
    /// the flip left behind.
    ///
    /// A machine's own transitions emit their repairs as events, the way `i`
    /// collapses the selection on the way into helix insert. An event-driven
    /// flip never reaches that path, so the repair has to happen here. The one
    /// that bites is leaving a block caret for a bar caret: a block policy rests
    /// as a min-width-1 selection, and `insert_char` deletes the selection
    /// before inserting, so the first keystroke would replace the covered
    /// grapheme.
    ///
    /// Stated over the `RestPolicy` rather than per machine, so helix
    /// normal/select and vi visual are one rule instead of three cases, and a
    /// future machine inherits it.
    fn change_edit_mode(&mut self, event: ReedlineEvent) -> EventStatus {
        let before = self.edit_mode.edit_mode().rest_policy();
        let status = self.edit_mode.handle_mode_specific_event(event);
        let after = self.edit_mode.edit_mode().rest_policy();
        if before.is_block() && !after.is_block() {
            // `run_edit_commands` re-syncs the policy from the mode the machine
            // now reports, so this resolves under `after`. Collapsing under the
            // block policy being left would re-widen the cursor and undo it.
            // Backward is the edge `i` lands on.
            self.run_edit_commands(&[EditCommand::CollapseSelection(Direction::Backward)]);
        }
        status
    }

    fn handle_mouse_click(&mut self, column: u16, row: u16) -> Result<()> {
        let snapshot = match &self.last_render_snapshot {
            Some(snapshot) => snapshot,
            None => return Ok(()),
        };
        if self.input_mode != InputMode::Regular || self.menus.iter().any(|m| m.is_active()) {
            return Ok(());
        }
        let buffer = self.editor.get_buffer();
        if let Some(offset) = self.painter.screen_to_buffer_offset(snapshot, column, row) {
            if buffer.is_char_boundary(offset) {
                self.editor.edit_buffer(
                    |buf| buf.set_insertion_point(offset),
                    UndoBehavior::MoveCursor,
                );
            }
        }
        Ok(())
    }

    fn active_menu(&mut self) -> Option<&mut ReedlineMenu> {
        self.menus.iter_mut().find(|menu| menu.is_active())
    }

    fn deactivate_menus(&mut self) {
        // Nothing is left to complete into.
        self.deferred_menu_completion = None;
        self.menus
            .iter_mut()
            .for_each(|menu| menu.menu_event(MenuEvent::Deactivate));
    }

    fn previous_history(&mut self) -> io::Result<()> {
        self.history_cursor_on_excluded = false;
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
            // On `Err` the next press retries on the fresh cursor; no rollback.
            self.history_cursor.back(self.history.as_ref())?;
        }
        self.update_buffer_from_history();
        self.editor.move_to_start(false);
        self.editor.move_to_line_end(false);
        // History navigation positions the cursor outside the command path, so
        // settle it under the rest policy (vi-normal must not rest past the line).
        self.editor.commit_cursor();
        self.editor
            .update_undo_state(UndoBehavior::HistoryNavigation);
        Ok(())
    }

    fn next_history(&mut self) -> io::Result<()> {
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
            self.history_cursor.forward(self.history.as_ref())?;

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
        // See `previous_history`: settle the out-of-band cursor under the policy.
        self.editor.commit_cursor();
        self.editor
            .update_undo_state(UndoBehavior::HistoryNavigation);
        Ok(())
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
    fn run_history_commands(&mut self, commands: &[EditCommand]) -> io::Result<()> {
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
                    self.history_cursor.back(self.history.as_mut())?;
                }
                EditCommand::Backspace => {
                    let navigation = self.history_cursor.get_navigation();

                    if let HistoryNavigationQuery::SubstringSearch(substring) = navigation {
                        let new_substring = text_manipulation::remove_last_grapheme(&substring);

                        self.history_cursor = HistoryCursor::new(
                            HistoryNavigationQuery::SubstringSearch(new_substring.to_string()),
                            self.get_history_session_id(),
                        );
                        self.history_cursor.back(self.history.as_mut())?
                    }
                }
                _ => {
                    self.input_mode = InputMode::Regular;
                }
            }
        }
        Ok(())
    }

    /// Set the buffer contents for history traversal/search in the standard prompt
    ///
    /// When using the up/down traversal or fish/zsh style prefix search update the main line buffer accordingly.
    /// Not used for the separate modal reverse search!
    fn update_buffer_from_history(&mut self) {
        if self.history_cursor_on_excluded {
            if let Some(item) = &self.history_excluded_item {
                self.editor
                    .set_buffer(item.command_line.clone(), UndoBehavior::HistoryNavigation);
            }
            return;
        }

        match self.history_cursor.get_navigation() {
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
            HistoryNavigationQuery::PrefixSearch(prefix)
            | HistoryNavigationQuery::SubstringSearch(prefix) => {
                let buffer = self.history_cursor.string_at_cursor().unwrap_or(prefix);
                self.editor
                    .set_buffer(buffer, UndoBehavior::HistoryNavigation);
            }
        }
    }

    /// Executes [`EditCommand`] actions by modifying the internal state appropriately. Does not output itself.
    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        if self.input_mode == InputMode::HistoryTraversal {
            self.input_mode = InputMode::Regular;
        }
        self.apply_edit_commands(commands);
    }

    /// [`run_edit_commands`](Self::run_edit_commands) without ending history
    /// traversal, for the engine's own line moves inside a recalled entry.
    fn apply_edit_commands(&mut self, commands: &[EditCommand]) {
        // Adopt the current edit mode's rest policy so these commands resolve
        // under it (e.g. block-caret selection geometry) — but *without*
        // committing the cursor first. A commit here would apply the policy's
        // resting rule (e.g. `OnGrapheme` pulling an at-end point back) before
        // the commands run, double-stepping a mode-transition backstep like the
        // vi `Esc`→normal `MoveLeft`. The commands settle the cursor themselves,
        // and the pre-paint `set_edit_mode` makes the final commit.
        self.editor.sync_edit_mode(self.edit_mode.edit_mode());

        // Run the commands over the edit buffer
        for command in commands {
            if let Some(command) = self.auto_pair_command(command) {
                self.editor.run_edit_command(&command);
                continue;
            }

            self.editor.run_edit_command(command);
        }
    }

    fn auto_pair_command(&self, command: &EditCommand) -> Option<EditCommand> {
        let auto_pairs = self.auto_pairs.as_ref()?;

        // Resolve which auto-pair action (if any) `command` would trigger, along
        // with the pair it acts on and the `EditCommand` that would replace it.
        // The search order matters: for `InsertChar`, closers are checked before
        // openers, so a character that is configured as both a closer of one pair
        // and an opener of another resolves based on whether the cursor currently
        // sits on the closer, not on the order pairs were registered in.
        let (pair, action, converted) = match command {
            EditCommand::InsertChar(ch) => {
                let closer_at_cursor = auto_pairs
                    .closing_pair(*ch)
                    .filter(|(_, close)| self.editor.is_auto_pair_closer_at_cursor(*close));

                if let Some(pair) = closer_at_cursor {
                    (
                        pair,
                        AutoPairAction::SkipExistingCloser,
                        EditCommand::MoveRight { select: false },
                    )
                } else if let Some((open, close)) = auto_pairs.opening_pair(*ch) {
                    (
                        (open, close),
                        AutoPairAction::Open,
                        EditCommand::InsertPair { open, close },
                    )
                } else {
                    return None;
                }
            }
            EditCommand::Backspace => {
                let pair = auto_pairs.pairs().find(|(open, close)| {
                    self.editor.is_empty_auto_pair_at_cursor(*open, *close)
                })?;
                (
                    pair,
                    AutoPairAction::BackspacePair,
                    EditCommand::BackspacePair {
                        open: pair.0,
                        close: pair.1,
                    },
                )
            }
            _ => return None,
        };

        // Give the highlighter a chance to veto the action before committing to
        // it. All three actions pass through this same gate: returning `false`
        // means "run the original command verbatim", handled by the caller
        // treating `None` from this function as a pass-through.
        let buffer = self.editor.get_buffer();
        let insertion_point = self.editor.insertion_point();
        let selection = self.editor.get_selection().map(|(start, end)| start..end);
        let context = AutoPairContext::new(buffer, insertion_point, pair, selection, action);

        if self.highlighter.should_auto_pair(&context) {
            Some(converted)
        } else {
            None
        }
    }

    fn up_command(&mut self) -> io::Result<()> {
        // If we're at the top, then:
        if self.editor.is_cursor_at_first_line() {
            // If we're at the top, move to previous history
            self.previous_history()
        } else {
            // Through `apply_edit_commands` so the cursor settles under the mode's
            // rest policy — a bare `editor.move_line_up` skips the commit boundary,
            // leaving a vi-normal caret past the last grapheme on a short line.
            self.apply_edit_commands(&[EditCommand::MoveLineUp { select: false }]);
            Ok(())
        }
    }

    fn down_command(&mut self) -> io::Result<()> {
        // If we're at the top, then:
        if self.editor.is_cursor_at_last_line() {
            // If we're at the top, move to previous history
            self.next_history()
        } else {
            // See `up_command`: settle under the rest policy via the commit boundary.
            self.apply_edit_commands(&[EditCommand::MoveLineDown { select: false }]);
            Ok(())
        }
    }

    /// Checks if hints should be displayed and are able to be completed
    fn hints_active(&self) -> bool {
        !self.hide_hints && matches!(self.input_mode, InputMode::Regular)
    }

    /// Accept a trailing history hint (full hint or next word) by appending it at
    /// the buffer end. `Handled` only when a non-empty hint applies: hints active,
    /// cursor at the buffer end, no menu open. Appending positions past the last
    /// grapheme first — a block caret (vi normal) rests *on* it, so a plain insert
    /// would split it.
    fn accept_history_hint(&mut self, hint: Option<String>) -> EventStatus {
        let Some(hint) = hint else {
            return EventStatus::Inapplicable;
        };
        if self.hints_active()
            && self.editor.is_cursor_at_buffer_end()
            && !hint.is_empty()
            && self.active_menu().is_none()
        {
            self.editor.prepare_append_at_buffer_end();
            self.run_edit_commands(&[EditCommand::InsertString(hint)]);
            EventStatus::Handled
        } else {
            EventStatus::Inapplicable
        }
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

    /// Expands an abbreviation at the word before the cursor, if any exists
    ///
    /// Calls [`Highlighter::should_expand_abbr`] with [`AbbrExpandContext::WordAbbreviation`]
    /// to decide whether expansion is permitted at the cursor position
    fn try_expand_abbreviation_at_cursor(&mut self, submitted: bool) -> Option<ReedlineEvent> {
        let buffer = self.editor.get_buffer();
        let cursor_position_in_buffer = self.editor.insertion_point();
        if cursor_position_in_buffer == 0 {
            return None;
        }

        let (offset, suffix) = match submitted {
            true => (0, ""),   // expand on <enter>
            false => (1, " "), // expand on <space>
        };

        // `offset` is a raw byte count (0 on <enter>, 1 on <space>), so
        // `cursor_position_in_buffer - offset` can land inside a multi-byte
        // UTF-8 char sitting just before the cursor (e.g. pasted CJK text).
        // Floor it down to the nearest char boundary before slicing to avoid
        // a panic.
        let word_end =
            crate::menu_functions::floor_char_boundary(buffer, cursor_position_in_buffer - offset);
        let prefix = &buffer[..word_end];
        let word_start = prefix
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0); // byte offset of word start

        if word_start >= word_end {
            // The first char in the buffer is a space or there are consecutive spaces
            return None;
        }

        if submitted
            && buffer[word_end..]
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_whitespace())
        {
            // The cursor is in the middle of a word, e.g. "hello|world"
            return None;
        }

        if !self.highlighter.should_expand_abbr(
            buffer,
            word_start,
            AbbrExpandContext::WordAbbreviation,
        ) {
            return None;
        }

        let word = &buffer[word_start..word_end];
        if let Some(expansion) = self.abbreviations.get(word) {
            return Some(ReedlineEvent::Edit(vec![
                EditCommand::MoveToPosition {
                    position: word_start,
                    select: false,
                },
                EditCommand::MoveToPosition {
                    // Select through the cursor, not just the end of the word, so
                    // the triggering space (already inserted on a <space> expansion)
                    // is replaced rather than left beside the inserted suffix.
                    position: cursor_position_in_buffer,
                    select: true,
                },
                EditCommand::InsertString(format!("{}{}", expansion, suffix)),
            ]));
        }

        None
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

        if !self.highlighter.should_expand_abbr(
            buffer,
            parsed.remainder.len(),
            AbbrExpandContext::BangExpansion,
        ) {
            return None;
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
                            parsed.prefix.unwrap_or_default().to_string(),
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
                // Capture the prompt's screen range so that an editor
                // that leaves the cursor untouched (e.g. an editor that
                // uses the alternate screen only) re-uses the existing
                // prompt rows instead of starting a new prompt a row
                // below the old one.
                let suspended_state = self.painter.state_before_suspension();
                {
                    let mut child = command.spawn()?;
                    // The child owns the tty now; invalidate eagerly so
                    // any `?` early-return below still leaves the
                    // painter in a safe state.
                    self.painter.invalidate_prompt_start_row();
                    child.wait()?;
                }

                // On the success path, re-initialize position and size
                // (covers a resize-during-editor with no SIGWINCH). If
                // the editor moved the cursor out of the prompt's rows
                // (it printed output), a fresh prompt starts below that
                // output. On query failure, the eager invalidate above
                // is our floor — losing the size refresh is acceptable;
                // losing the user's edited buffer below is not.
                let _ = self
                    .painter
                    .initialize_prompt_position(Some(&suspended_state));

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
            // With a cursor-cell style configured, the head cell gets it and
            // the selection style covers the rest of the range.
            match self
                .visual_selection_cursor_style
                .zip(self.editor.selection_head_cell())
            {
                Some((cursor_style, (cell_start, cell_end))) => {
                    if from < cell_start {
                        styled_text.style_range(from, cell_start, self.visual_selection_style);
                    }
                    styled_text.style_range(cell_start, cell_end, cursor_style);
                    if cell_end < to {
                        styled_text.style_range(cell_end, to, self.visual_selection_style);
                    }
                }
                None => styled_text.style_range(from, to, self.visual_selection_style),
            }
        }

        let (before_cursor, after_cursor) = styled_text.render_around_insertion_point(
            cursor_position_in_buffer,
            prompt,
            self.use_ansi_coloring,
            self.painter.semantic_markers(),
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
                // A menu still waiting on its first answer stays off screen, so a Tab
                // resolving to one suggestion never draws a menu it takes away again.
                if menu.is_visible() {
                    lines.prompt_indicator = menu.indicator().to_owned().into();
                }
                // If the menu requires the cursor position, update it (ide menu)
                let cursor_pos = lines.cursor_pos(self.painter.screen_width());
                menu.set_cursor_pos(cursor_pos);

                menu.update_working_details(
                    &mut self.editor,
                    self.completer.as_mut(),
                    self.history.as_ref(),
                    &self.painter,
                );

                // That update is where a first answer lands and ends the opening phase,
                // so ask again: the painter picks the menu to draw below, and an
                // indicator saying otherwise would draw its rows under the ordinary
                // prompt. Reading it twice is the price of the loop: the indicator sets
                // the prompt width that positions the cursor, which the update consumes,
                // so on the frame a menu opens that width lags by one paint.
                if menu.is_visible() {
                    lines.prompt_indicator = menu.indicator().to_owned().into();
                }
            }
        }

        let menu = self.menus.iter().find(|menu| menu.is_visible());

        self.painter.repaint_buffer(
            prompt,
            &lines,
            self.prompt_edit_mode(),
            menu,
            self.use_ansi_coloring,
            &self.cursor_shapes,
        )?;

        if self.mouse_click_mode.is_enabled() {
            if let Some(layout) = &self.painter.last_layout {
                let buffer = self.editor.get_buffer();
                let (raw_before, raw_after) = buffer.split_at(cursor_position_in_buffer);
                self.last_render_snapshot = Some(
                    self.painter
                        .render_snapshot(&lines, menu, raw_before, raw_after, layout),
                );
            }
        } else {
            self.last_render_snapshot = None;
        }

        Ok(())
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

    /// Sets the poll interval used when features that require periodic processing
    /// are active (e.g., external printer, idle callback).
    ///
    /// This controls how frequently Reedline yields control back to these features
    /// while waiting for user input. The default is 100ms.
    ///
    /// Common values are 33ms (~30fps) for UI updates or 100ms for less frequent tasks.
    ///
    /// Note: This setting only takes effect when an external printer or idle callback
    /// is configured. Without these features, Reedline blocks until input is received.
    ///
    /// # Example
    /// ```no_run
    /// use std::time::Duration;
    /// use reedline::Reedline;
    ///
    /// let editor = Reedline::create()
    ///     .with_poll_interval(Duration::from_millis(50));
    /// ```
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Sets an idle callback that is called periodically while waiting for user input.
    ///
    /// This is useful for applications that need to process external events
    /// (such as GUI updates, network events, or timer-based operations) while
    /// the user is typing or the editor is waiting for input.
    ///
    /// Use [`with_poll_interval`](Self::with_poll_interval) to control how frequently
    /// the callback is invoked (default: 100ms).
    ///
    /// # Example
    /// ```no_run
    /// use std::time::Duration;
    /// use reedline::Reedline;
    ///
    /// let editor = Reedline::create()
    ///     .with_poll_interval(Duration::from_millis(33))
    ///     .with_idle_callback(Box::new(|| {
    ///         // Process external events here
    ///     }));
    /// ```
    pub fn with_idle_callback(mut self, callback: Box<dyn FnMut() + Send>) -> Self {
        self.idle_callback = Some(callback);
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

            let excluded = self
                .history_exclusion_prefix
                .as_ref()
                .is_some_and(|prefix| buffer.starts_with(prefix));

            let saved = if excluded {
                None
            } else {
                match self.history.save(entry.clone()) {
                    Ok(saved) => Some(saved),
                    Err(err) => {
                        // Ran but not stored: the excluded shape. Keep the line,
                        // stash the error for `take_history_save_error`.
                        self.history_save_error = Some(err);
                        None
                    }
                }
            };

            match saved {
                Some(saved) => {
                    self.history_last_run_id = saved.id;
                    self.history_excluded_item = None;
                }
                None => {
                    entry.id = Some(Self::FILTERED_ITEM_ID);
                    self.history_last_run_id = entry.id;
                    self.history_excluded_item = Some(entry);
                }
            }
        }
        self.run_edit_commands(&[EditCommand::Clear]);
        self.editor.reset_undo_stack();

        Ok(EventStatus::Exits(Signal::Success(buffer)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_extensions::semantic_prompt::PromptKind;
    use crate::{
        ColumnarMenu, CompletionOrigin, CompletionResult, DefaultPrompt, MenuBuilder, PromptViMode,
        Span, Suggestion,
    };
    use rstest::rstest;

    fn seam_engine(edit_mode: Box<dyn EditMode>) -> Reedline {
        let mut rl = Reedline::create().with_edit_mode(edit_mode);
        rl.painter.force_prompt_anchored_for_test(0);
        rl
    }

    fn drive(rl: &mut Reedline, keys: &[KeyEvent]) {
        let prompt = DefaultPrompt::default();
        let events = keys.iter().copied().map(Event::Key).collect();
        let _ = rl.process_input_batch(&prompt, events).expect("batch ok");
    }

    fn ch(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Drive each key as its own input batch, so vi mode transitions settle
    /// between presses the way real keystrokes arrive.
    fn type_each(rl: &mut Reedline, keys: &[KeyEvent]) {
        for k in keys {
            drive(rl, &[*k]);
        }
    }

    fn auto_pair_engine(pairs: &[(char, char)]) -> Reedline {
        Reedline::create().with_auto_pairs(AutoPairs::new(pairs.iter().copied()))
    }

    #[test]
    fn auto_pairs_disabled_keeps_literal_typing() {
        let mut rl = Reedline::create();
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_ignore_unconfigured_openers() {
        let mut rl = auto_pair_engine(&[('[', ']')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_insert_pair_and_continue_inside() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);

        rl.run_edit_commands(&[EditCommand::InsertChar('a')]);

        assert_eq!(rl.editor.get_buffer(), "(a)");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_skip_existing_closer() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('('), EditCommand::InsertChar(')')]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_backspace_removes_empty_pair_as_one_undo_step() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        rl.run_edit_commands(&[EditCommand::Backspace]);

        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);

        rl.run_edit_commands(&[EditCommand::Undo]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_backspace_after_buffer_replacement_removes_empty_pair() {
        let mut rl = auto_pair_engine(&[('(', ')')]);

        // A history/menu replacement bypasses the key event that originally
        // created the pair. Backspace must still inspect the live buffer and
        // remove the empty pair as one operation.
        rl.run_edit_commands(&[
            EditCommand::InsertString("stale input".into()),
            EditCommand::Clear,
            EditCommand::InsertString("()".into()),
            EditCommand::MoveLeft { select: false },
        ]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);

        rl.run_edit_commands(&[EditCommand::Backspace]);

        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_backspace_after_pair_newline_keeps_closer() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);

        // The newline separates the pair, so Backspace must remove only the
        // newline rather than treating the surrounding characters as an empty
        // pair.
        rl.run_edit_commands(&[EditCommand::InsertNewline]);
        assert_eq!(rl.editor.get_buffer(), "(\n)");
        assert_eq!(rl.editor.insertion_point(), 2);

        rl.run_edit_commands(&[EditCommand::Backspace]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_wrap_selection_with_opener() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[
            EditCommand::InsertString("abc".into()),
            EditCommand::MoveToStart { select: false },
            EditCommand::MoveRight { select: true },
            EditCommand::MoveRight { select: true },
            EditCommand::MoveRight { select: true },
            EditCommand::InsertChar('('),
        ]);

        assert_eq!(rl.editor.get_buffer(), "(abc)");
        assert_eq!(rl.editor.get_selection(), None);
        assert_eq!(rl.editor.insertion_point(), 5);
    }

    #[test]
    fn auto_pairs_do_not_rewrite_insert_string() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertString("()".into())]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_support_same_character_pairs() {
        let mut rl = auto_pair_engine(&[('"', '"')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('"')]);

        assert_eq!(rl.editor.get_buffer(), "\"\"");
        assert_eq!(rl.editor.insertion_point(), 1);

        rl.run_edit_commands(&[EditCommand::InsertChar('"')]);

        assert_eq!(rl.editor.get_buffer(), "\"\"");
        assert_eq!(rl.editor.insertion_point(), 2);

        rl.run_edit_commands(&[
            EditCommand::MoveLeft { select: false },
            EditCommand::Backspace,
        ]);

        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    // --- `Highlighter::should_auto_pair` veto -------------------------------

    /// Vetoes exactly one [`AutoPairAction`], letting the other two proceed
    /// unmodified — used to prove the three actions are gated independently.
    struct VetoActionHighlighter(AutoPairAction);

    impl Highlighter for VetoActionHighlighter {
        fn highlight(&self, _line: &str, _cursor: usize) -> crate::StyledText {
            crate::StyledText::new()
        }

        fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
            context.action() != self.0
        }
    }

    fn auto_pair_engine_with_veto(pairs: &[(char, char)], vetoed: AutoPairAction) -> Reedline {
        auto_pair_engine(pairs).with_highlighter(Box::new(VetoActionHighlighter(vetoed)))
    }

    #[test]
    fn auto_pairs_veto_open_falls_back_to_literal_insert_char() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::Open);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        // Fallback must be exactly `InsertChar('(')`, not `InsertPair`.
        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_veto_open_does_not_affect_skip_over_or_backspace_pair() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::Open);

        // Build "(a)" directly (bypassing the vetoed `Open` action) and place
        // the cursor right before the closer.
        rl.run_edit_commands(&[
            EditCommand::InsertString("(a)".into()),
            EditCommand::MoveLeft { select: false },
        ]);
        assert_eq!(rl.editor.insertion_point(), 2);

        // `SkipExistingCloser` is not vetoed here, so it must still fire.
        rl.run_edit_commands(&[EditCommand::InsertChar(')')]);
        assert_eq!(rl.editor.get_buffer(), "(a)");
        assert_eq!(rl.editor.insertion_point(), 3);

        // Rebuild an empty pair the same way and confirm `BackspacePair` still
        // collapses it as one step even though `Open` is vetoed.
        rl.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("()".into()),
            EditCommand::MoveLeft { select: false },
            EditCommand::Backspace,
        ]);
        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_veto_skip_existing_closer_falls_back_to_literal_insert_char() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::SkipExistingCloser);

        rl.run_edit_commands(&[
            EditCommand::InsertString("(a)".into()),
            EditCommand::MoveLeft { select: false },
        ]);
        assert_eq!(rl.editor.insertion_point(), 2);

        // Decision order: even though the cursor sits on an existing closer
        // (which would normally win unconditionally), the veto is consulted
        // and a literal `)` is inserted instead of skipping over.
        rl.run_edit_commands(&[EditCommand::InsertChar(')')]);
        assert_eq!(rl.editor.get_buffer(), "(a))");
        assert_eq!(rl.editor.insertion_point(), 3);

        // `Open` is unaffected by this veto.
        rl.run_edit_commands(&[EditCommand::Clear, EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_veto_backspace_pair_falls_back_to_plain_backspace() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::BackspacePair);

        // `Open` is not vetoed, so this still produces a real empty pair.
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);

        // `BackspacePair` is vetoed: fallback is a plain `Backspace`, deleting
        // only the grapheme to the left of the cursor.
        rl.run_edit_commands(&[EditCommand::Backspace]);
        assert_eq!(rl.editor.get_buffer(), ")");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_veto_open_with_selection_replaces_selection_literally() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::Open);
        rl.run_edit_commands(&[
            EditCommand::InsertString("abc".into()),
            EditCommand::MoveToStart { select: false },
            EditCommand::MoveRight { select: true },
            EditCommand::MoveRight { select: true },
            EditCommand::MoveRight { select: true },
            EditCommand::InsertChar('('),
        ]);

        // Contrast with `auto_pairs_wrap_selection_with_opener` (no veto),
        // which produces "(abc)" with the cursor at 5: vetoing `Open` must
        // instead replace the selection with a literal '(', same as ordinary
        // typing over a selection.
        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.get_selection(), None);
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_veto_sees_buffer_and_cursor_after_preceding_commands_in_batch() {
        // The context passed to `should_auto_pair` must reflect live state —
        // not a stale snapshot taken before other commands in the same batch
        // (e.g. a history navigation or completion insert) ran.
        let seen: std::sync::Arc<std::sync::Mutex<Vec<(String, usize, AutoPairAction)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        struct RecordingHighlighter {
            seen: std::sync::Arc<std::sync::Mutex<Vec<(String, usize, AutoPairAction)>>>,
        }

        impl Highlighter for RecordingHighlighter {
            fn highlight(&self, _line: &str, _cursor: usize) -> crate::StyledText {
                crate::StyledText::new()
            }

            fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
                self.seen.lock().unwrap().push((
                    context.buffer().to_string(),
                    context.insertion_point(),
                    context.action(),
                ));
                true
            }
        }

        let mut rl = auto_pair_engine(&[('(', ')')])
            .with_highlighter(Box::new(RecordingHighlighter { seen: seen.clone() }));

        // Replace the whole buffer (as a history navigation or menu accept
        // would) and immediately type an opener in the same batch.
        rl.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("echo hi".into()),
            EditCommand::InsertChar('('),
        ]);

        let recorded = seen.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "echo hi");
        assert_eq!(recorded[0].1, "echo hi".len());
        assert_eq!(recorded[0].2, AutoPairAction::Open);
    }

    // Regression: `auto_pair_command` used to derive the context's selection
    // by comparing `selection_anchor()` against `insertion_point()` directly.
    // Under a forward vi-normal (block) selection those two disagree with the
    // actual selected range: `insertion_point()` is the *caret*, one grapheme
    // back from the cursor's `head` (see `Editor::insertion_point` /
    // `Cursor::caret`), while `Editor::get_selection()` — the range
    // `insert_pair` (and thus the real `InsertPair` wrap) actually uses —
    // reports `cursor.start()..cursor.end()` with `end()` at the widened
    // `head`. The old code silently dropped the selection's last grapheme
    // from the context it handed to `should_auto_pair`.
    //
    // This mirrors the exact selection shape pinned by
    // `vi_normal_selection_cut_is_inclusive` in `core_editor::editor`'s own
    // tests: from "hello" at position 0, two forward `MoveRight { select:
    // true }` steps land the head on 'l' (byte 2) but widen the selection to
    // byte 3 to cover it.
    #[test]
    fn auto_pairs_context_selection_matches_vi_block_forward_selection() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<std::ops::Range<usize>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        struct RecordingSelectionHighlighter {
            seen: std::sync::Arc<std::sync::Mutex<Vec<std::ops::Range<usize>>>>,
        }

        impl Highlighter for RecordingSelectionHighlighter {
            fn highlight(&self, _line: &str, _cursor: usize) -> crate::StyledText {
                crate::StyledText::new()
            }

            fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
                if let Some(selection) = context.selection() {
                    self.seen.lock().unwrap().push(selection);
                }
                true
            }
        }

        // A fixed `EditMode` that always reports Vi-normal, so `run_edit_commands`'s
        // `sync_edit_mode` (which re-adopts `Reedline`'s own edit mode on every
        // call, independent of whatever `Editor::set_edit_mode` was last told)
        // does not flip the block-caret rest policy back to `Bar` behind our
        // back between the selection setup below and the `InsertChar` that
        // exercises `auto_pair_command`.
        struct AlwaysViNormal;
        impl EditMode for AlwaysViNormal {
            fn parse_event(&mut self, _e: ReedlineRawEvent) -> ReedlineEvent {
                ReedlineEvent::None
            }
            fn edit_mode(&self) -> PromptEditMode {
                PromptEditMode::Vi(PromptViMode::Normal)
            }
        }

        let mut rl = auto_pair_engine(&[('(', ')')])
            .with_highlighter(Box::new(RecordingSelectionHighlighter {
                seen: seen.clone(),
            }))
            .with_edit_mode(Box::new(AlwaysViNormal));

        rl.run_edit_commands(&[EditCommand::InsertString("hello".into())]);
        rl.editor.run_edit_command(&EditCommand::MoveToPosition {
            position: 0,
            select: false,
        });
        rl.editor
            .run_edit_command(&EditCommand::MoveRight { select: true });
        rl.editor
            .run_edit_command(&EditCommand::MoveRight { select: true });

        // Sanity-check the premise: a forward block selection whose head sits
        // one grapheme past what `insertion_point()` alone would suggest.
        assert_eq!(rl.editor.insertion_point(), 2);
        let expected_selection = rl.editor.get_selection().expect("selection active");
        assert_eq!(expected_selection, (0, 3));

        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        let recorded = seen.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0],
            expected_selection.0..expected_selection.1,
            "context selection must match Editor::get_selection(), not a range \
             derived from insertion_point()"
        );

        // `insert_pair` wraps the exact same range `get_selection()` reported,
        // so the buffer confirms the context wasn't merely coincidentally
        // correct: 'l' at byte 2 must be inside the pair.
        assert_eq!(rl.editor.get_buffer(), "(hel)lo");
    }

    /// A small context-sensitive policy used to exercise the consumer-facing
    /// `Highlighter` hook. It demonstrates that syntax-aware consumers can
    /// veto opening a pair while leaving the other auto-pair actions alone.
    struct ContextAwareAutoPairHighlighter;

    impl ContextAwareAutoPairHighlighter {
        fn inside_unclosed_quote(buffer: &str, point: usize, quote: char) -> bool {
            let mut in_quote = false;
            let mut escaped = false;
            for ch in buffer[..point].chars() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match ch {
                    '\\' => escaped = true,
                    c if c == quote => in_quote = !in_quote,
                    _ => {}
                }
            }
            in_quote
        }
    }

    impl Highlighter for ContextAwareAutoPairHighlighter {
        fn highlight(&self, _line: &str, _cursor: usize) -> crate::StyledText {
            crate::StyledText::new()
        }

        fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
            if context.action() != AutoPairAction::Open {
                return true;
            }

            let (open, close) = context.pair();
            let buffer = context.buffer();
            let point = context.insertion_point();

            if open == close && Self::inside_unclosed_quote(buffer, point, open) {
                return false;
            }

            true
        }
    }

    fn context_aware_auto_pair_engine() -> Reedline {
        auto_pair_engine(&[('(', ')'), ('"', '"')])
            .with_highlighter(Box::new(ContextAwareAutoPairHighlighter))
    }

    #[test]
    fn context_policy_allows_non_quote_pair_inside_unclosed_region() {
        let mut rl = context_aware_auto_pair_engine();
        rl.run_edit_commands(&[EditCommand::InsertString("\"abc".into())]);

        // A non-quote pair is allowed even inside an unclosed region.
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "\"abc()");
        assert_eq!(rl.editor.insertion_point(), 5);
    }

    #[test]
    fn context_policy_vetoes_same_delimiter_inside_unclosed_region() {
        let mut rl = context_aware_auto_pair_engine();
        rl.run_edit_commands(&[EditCommand::InsertString("\"".into())]);
        assert_eq!(rl.editor.insertion_point(), 1);

        // Typing the same quote again would normally open a new pair (cursor
        // is at the end of the buffer), but we are inside an unclosed string
        // of that same quote kind, so it must close it literally instead.
        rl.run_edit_commands(&[EditCommand::InsertChar('"')]);
        assert_eq!(rl.editor.get_buffer(), "\"\"");
        assert_eq!(rl.editor.insertion_point(), 2, "literal insert advances past the closing quote, unlike a paired insert which would leave the cursor at 1");
    }

    // --- undo granularity across all six action x veto combinations --------

    #[test]
    fn auto_pairs_undo_granularity_open_not_vetoed() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "()");

        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_undo_granularity_open_vetoed() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::Open);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        assert_eq!(rl.editor.get_buffer(), "(");

        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_undo_granularity_skip_existing_closer_not_vetoed() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        rl.run_edit_commands(&[EditCommand::InsertChar(')')]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 2);

        // The skip-over is a plain cursor move (`MoveRight`), and cursor
        // moves never open their own undo boundary in reedline — they merge
        // into whatever edit precedes them (verified directly against plain
        // `InsertChar` + `MoveLeft` with no auto-pairing involved). So one
        // `Undo` here reverts the *pair insertion* too, landing back at the
        // pre-`Open` empty buffer, not at the interim "()" state.
        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(rl.editor.insertion_point(), 0);
    }

    #[test]
    fn auto_pairs_undo_granularity_skip_existing_closer_vetoed() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::SkipExistingCloser);
        rl.run_edit_commands(&[
            EditCommand::InsertString("(a)".into()),
            EditCommand::MoveLeft { select: false },
        ]);
        rl.run_edit_commands(&[EditCommand::InsertChar(')')]);
        assert_eq!(rl.editor.get_buffer(), "(a))");

        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "(a)");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_undo_granularity_backspace_pair_not_vetoed() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        rl.run_edit_commands(&[EditCommand::Backspace]);
        assert_eq!(rl.editor.get_buffer(), "");

        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn auto_pairs_undo_granularity_backspace_pair_vetoed() {
        let mut rl = auto_pair_engine_with_veto(&[('(', ')')], AutoPairAction::BackspacePair);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);
        rl.run_edit_commands(&[EditCommand::Backspace]);
        assert_eq!(rl.editor.get_buffer(), ")");

        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    // --- edge cases: same-char pairs, overlaps, multi-line, grapheme -------

    #[test]
    fn auto_pairs_overlapping_pair_definitions_resolve_by_search_order() {
        // 'b' is both a closer (of `a`/`b`) and an opener (of `b`/`c`).
        // Closers are searched before openers, but only the cursor-at-closer
        // check short-circuits to a skip; otherwise the opener check runs.
        let mut rl = auto_pair_engine(&[('a', 'b'), ('b', 'c')]);

        // Not sitting on an existing 'b' closer (buffer is empty), so 'b' is
        // treated as an opener of the second pair.
        rl.run_edit_commands(&[EditCommand::InsertChar('b')]);
        assert_eq!(rl.editor.get_buffer(), "bc");
        assert_eq!(rl.editor.insertion_point(), 1);

        // Now place the cursor right before an existing 'b', which is also
        // configured as the closer of the first pair — skip-over wins.
        rl.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("ab".into()),
            EditCommand::MoveLeft { select: false },
        ]);
        assert_eq!(rl.editor.insertion_point(), 1);

        rl.run_edit_commands(&[EditCommand::InsertChar('b')]);
        assert_eq!(rl.editor.get_buffer(), "ab");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_multiline_and_crlf_buffer() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.run_edit_commands(&[EditCommand::InsertString("line1\r\nline2".into())]);
        rl.run_edit_commands(&[EditCommand::InsertChar('(')]);

        assert_eq!(rl.editor.get_buffer(), "line1\r\nline2()");
        assert_eq!(rl.editor.insertion_point(), "line1\r\nline2(".len());
    }

    #[test]
    fn auto_pairs_skip_over_grapheme_with_combining_mark_after_closer() {
        // The adjacency check (`grapheme_right().starts_with(close)`) is
        // exercised here with the closer immediately followed by a combining
        // mark, so `close` and the mark form a single grapheme cluster.
        // This pins current behaviour; see written report for whether this
        // is considered correct.
        let mut rl = auto_pair_engine(&[('(', ')')]);
        let combining_acute = '\u{0301}';
        rl.run_edit_commands(&[EditCommand::InsertString(format!("(){combining_acute}"))]);
        // The trailing ')' + combining mark form a single grapheme cluster,
        // so one `MoveLeft` from the end lands right before it.
        rl.run_edit_commands(&[EditCommand::MoveLeft { select: false }]);
        assert_eq!(rl.editor.insertion_point(), 1);

        rl.run_edit_commands(&[EditCommand::InsertChar(')')]);

        // The whole grapheme cluster (closer + combining mark) is skipped
        // over in one motion, landing the cursor at the end of the buffer.
        assert_eq!(rl.editor.get_buffer(), format!("(){combining_acute}"));
        assert_eq!(
            rl.editor.insertion_point(),
            format!("(){combining_acute}").len()
        );
    }

    // --- vi / helix InsertChar replay ---------------------------------------

    #[test]
    fn vi_insert_mode_auto_pair_routes_through_veto() {
        let mut rl = seam_engine(Box::<crate::Vi>::default())
            .with_auto_pairs(AutoPairs::new([('(', ')')]))
            .with_highlighter(Box::new(VetoActionHighlighter(AutoPairAction::Open)));

        type_each(&mut rl, &[ch('(')]);

        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn vi_insert_mode_auto_pair_still_pairs_when_not_vetoed() {
        let mut rl =
            seam_engine(Box::<crate::Vi>::default()).with_auto_pairs(AutoPairs::new([('(', ')')]));

        type_each(&mut rl, &[ch('(')]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn helix_insert_mode_auto_pair_routes_through_veto() {
        let mut rl = seam_engine(Box::<crate::Helix>::default())
            .with_auto_pairs(AutoPairs::new([('(', ')')]))
            .with_highlighter(Box::new(VetoActionHighlighter(AutoPairAction::Open)));

        type_each(&mut rl, &[ch('(')]);

        assert_eq!(rl.editor.get_buffer(), "(");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    #[test]
    fn helix_insert_mode_auto_pair_still_pairs_when_not_vetoed() {
        let mut rl = seam_engine(Box::<crate::Helix>::default())
            .with_auto_pairs(AutoPairs::new([('(', ')')]));

        type_each(&mut rl, &[ch('(')]);

        assert_eq!(rl.editor.get_buffer(), "()");
        assert_eq!(rl.editor.insertion_point(), 1);
    }

    // --- `Event::Paste` must never be rewritten -----------------------------

    #[test]
    fn auto_pairs_balanced_paste_event_is_not_rewritten() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.painter.force_prompt_anchored_for_test(0);
        let prompt = DefaultPrompt::default();
        let _ = rl
            .process_input_batch(&prompt, vec![Event::Paste("(a)".into())])
            .expect("batch ok");

        assert_eq!(rl.editor.get_buffer(), "(a)");
        assert_eq!(rl.editor.insertion_point(), 3);
    }

    #[test]
    fn auto_pairs_unbalanced_paste_event_is_not_rewritten() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.painter.force_prompt_anchored_for_test(0);
        let prompt = DefaultPrompt::default();
        let _ = rl
            .process_input_batch(&prompt, vec![Event::Paste("(a".into())])
            .expect("batch ok");

        // Pasted text is inserted verbatim via `InsertString`, which never
        // goes through `auto_pair_command` — even though the pasted opener
        // has no matching closer.
        assert_eq!(rl.editor.get_buffer(), "(a");
        assert_eq!(rl.editor.insertion_point(), 2);
    }

    #[test]
    fn auto_pairs_do_not_rewrite_reverse_history_search_query() {
        let mut rl = auto_pair_engine(&[('(', ')')]);
        rl.enter_history_search();

        rl.handle_history_search_event(ReedlineEvent::Edit(vec![EditCommand::InsertChar('(')]))
            .expect("history search event handled");

        assert_eq!(rl.input_mode, InputMode::HistorySearch);
        assert_eq!(rl.editor.get_buffer(), "");
        assert_eq!(
            rl.history_cursor.get_navigation(),
            HistoryNavigationQuery::SubstringSearch("(".into())
        );
    }

    // FLIP SAFETY NET (Group C) — visual operability at the engine seam.
    // RED until the cursor-as-truth flip: `v` emits Esc which clears the
    // selection, so visual mode starts anchorless and `d` cuts nothing. The
    // flip makes the cursor an always-present range, so `v` then `d` deletes
    // the grapheme under the cursor. Valid under both models, so never wasted.
    #[test]
    fn v_then_d_deletes_cursor_grapheme() {
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        type_each(
            &mut rl,
            &[ch('a'), ch('b'), key(KeyCode::Esc), ch('v'), ch('d')],
        );
        assert_eq!(rl.editor.get_buffer(), "a");
    }

    #[test]
    fn v_extend_left_then_d_deletes_selection() {
        // Visual mode is min-width-1 and motions extend it: from "abc" the cursor
        // rests on 'c'; `v` selects it, `h` grows the selection left over 'b',
        // and `d` deletes both — leaving "a".
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        type_each(
            &mut rl,
            &[
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                ch('v'),
                ch('h'),
                ch('d'),
            ],
        );
        assert_eq!(rl.editor.get_buffer(), "a");
    }

    struct FlipToNormal {
        switched: bool,
    }
    impl EditMode for FlipToNormal {
        fn parse_event(&mut self, _e: ReedlineRawEvent) -> ReedlineEvent {
            self.switched = true;
            ReedlineEvent::None
        }
        fn edit_mode(&self) -> PromptEditMode {
            if self.switched {
                PromptEditMode::Vi(PromptViMode::Normal)
            }
            // OnGrapheme
            else {
                PromptEditMode::Vi(PromptViMode::Insert)
            }
        }
    }

    #[test]
    fn command_less_mode_transition_settles_cursor() {
        let mut rl = seam_engine(Box::new(FlipToNormal { switched: false }));
        rl.editor
            .set_buffer("ab".into(), UndoBehavior::CreateUndoPoint);
        rl.editor
            .edit_buffer(|b| b.set_insertion_point(2), UndoBehavior::MoveCursor); // at len, legal under Between
        drive(&mut rl, &[ch('x')]); // flipts to OnGrapheme, emits nothing
        assert_eq!(rl.current_insertion_point(), 1);
    }

    #[test]
    fn harness_drives_typed_chars_into_buffer() {
        // Smoke test: proves the seam harness runs the real batch pipeline
        // (parse_event -> handle_event -> repaint-to-sink) headlessly.
        let mut rl = seam_engine(Box::<crate::Emacs>::default());
        drive(&mut rl, &[ch('h'), ch('i')]);
        assert_eq!(rl.editor.get_buffer(), "hi");
        assert_eq!(rl.current_insertion_point(), 2);
    }

    #[test]
    fn immediately_accept_submits_without_hanging() {
        // Regression: the batch-processing call (which pushes the synthetic
        // Submit and returns the buffer) must run even in immediately_accept
        // mode. When it was gated behind `!immediately_accept`, read_line spun
        // forever instead of submitting.
        let mut rl = seam_engine(Box::<crate::Emacs>::default());
        rl.immediately_accept = true;
        rl.run_edit_commands(&[EditCommand::InsertString("hi".into())]);
        let prompt = DefaultPrompt::default();
        match rl.process_input_batch(&prompt, vec![]).expect("batch ok") {
            ControlFlow::Break(Signal::Success(buf)) => assert_eq!(buf, "hi"),
            other => panic!("expected immediate submit, got {other:?}"),
        }
    }

    #[test]
    fn reedline_is_send() {
        // `Reedline` must stay `Send` so it can be moved across threads.
        // The `Send` bound lives on the stored `Box<dyn Completer + Send>`
        // (engine + `ReedlineMenu`), not on the `Completer`/`Menu` traits
        // themselves, so this guards against that bound being dropped.
        fn assert_send<T: Send>() {}
        assert_send::<Reedline>();
    }

    #[test]
    fn test_cursor_position_after_multiline_history_navigation() {
        // Test for https://github.com/nushell/reedline/pull/899
        // Ensure that after navigating to a multiline history entry and then
        // running edit commands, the cursor doesn't jump unexpectedly.
        // The fix prevents set_buffer() from being called unnecessarily,
        // which would reset the insertion point.

        let mut reedline = Reedline::create();

        // Add a multiline entry to history
        let multiline_command = "echo 'line 1'\necho 'line 2'\necho 'line 3'";
        let history_item = HistoryItem::from_command_line(multiline_command);
        reedline
            .history
            .save(history_item)
            .expect("Failed to save history");

        // Navigate to previous history
        reedline.previous_history().expect("history ok");

        // Get the initial insertion point after history navigation
        let initial_insertion_point = reedline.current_insertion_point();

        // The buffer should contain our multiline command
        assert_eq!(reedline.current_buffer_contents(), multiline_command);

        // After the fix, previous_history() positions cursor at end of first line
        // (after move_to_start + move_to_line_end)
        let first_line_end = multiline_command.find('\n').unwrap();
        assert_eq!(initial_insertion_point, first_line_end);

        // Now simulate pressing the right arrow key, which should move cursor right
        // Without the fix, set_buffer() would be called and reset the insertion point,
        // causing the cursor to jump unexpectedly. With the fix, it stays where it is
        // and moves correctly.
        reedline.run_edit_commands(&[EditCommand::MoveRight { select: false }]);

        let after_move_insertion_point = reedline.current_insertion_point();

        // The cursor should have moved right by 1 from where it was
        assert_eq!(after_move_insertion_point, initial_insertion_point + 1);

        // The buffer should still be unchanged
        assert_eq!(reedline.current_buffer_contents(), multiline_command);
    }

    // --- history walk across a recalled multi-line entry (#1109 regression) ---

    /// History holds `older` then `one\ntwo` (newest).
    fn two_entry_history_engine() -> Reedline {
        let mut rl = seam_engine(Box::<Emacs>::default());
        for cmd in ["older", "one\ntwo"] {
            rl.history
                .save(HistoryItem::from_command_line(cmd))
                .expect("save history");
        }
        rl
    }

    #[test]
    fn down_inside_recalled_multiline_entry_keeps_walking_forward() {
        let mut rl = two_entry_history_engine();
        drive(&mut rl, &[key(KeyCode::Up)]);
        assert_eq!(rl.editor.get_buffer(), "one\ntwo", "setup");
        assert_eq!(rl.editor.insertion_point(), 3, "setup: end of line 1");

        drive(&mut rl, &[key(KeyCode::Down)]);
        assert_eq!(rl.editor.get_buffer(), "one\ntwo", "moves to line 2 first");
        assert!(rl.editor.insertion_point() > 3, "setup: on line 2");

        drive(&mut rl, &[key(KeyCode::Down)]);
        assert_eq!(
            rl.editor.get_buffer(),
            "",
            "from the last line, Down walks forward to the empty draft"
        );
    }

    #[test]
    fn up_inside_recalled_multiline_entry_keeps_walking_back() {
        let mut rl = two_entry_history_engine();
        drive(&mut rl, &[key(KeyCode::Up), key(KeyCode::Down)]);
        assert_eq!(rl.editor.get_buffer(), "one\ntwo", "setup");
        assert!(rl.editor.insertion_point() > 3, "setup: on line 2");

        drive(&mut rl, &[key(KeyCode::Up)]);
        assert_eq!(rl.editor.get_buffer(), "one\ntwo", "moves to line 1 first");
        assert!(rl.editor.insertion_point() <= 3, "setup: on line 1");

        drive(&mut rl, &[key(KeyCode::Up)]);
        assert_eq!(
            rl.editor.get_buffer(),
            "older",
            "from the first line, Up walks back to the older entry"
        );
    }

    // --- a history that refuses to save ---

    struct RefusingHistory;

    impl History for RefusingHistory {
        fn save(&mut self, _h: HistoryItem) -> crate::Result<HistoryItem> {
            Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                "refused",
            )))
        }
        fn load(&self, _id: HistoryItemId) -> crate::Result<HistoryItem> {
            unreachable!("not used")
        }
        fn count(&self, _query: SearchQuery) -> crate::Result<i64> {
            Ok(0)
        }
        fn search(&self, _query: SearchQuery) -> crate::Result<Vec<HistoryItem>> {
            Ok(vec![])
        }
        fn update(
            &mut self,
            _id: HistoryItemId,
            _updater: &dyn Fn(HistoryItem) -> HistoryItem,
        ) -> crate::Result<()> {
            unreachable!("not used")
        }
        fn clear(&mut self) -> crate::Result<()> {
            Ok(())
        }
        fn delete(&mut self, _h: HistoryItemId) -> crate::Result<()> {
            Ok(())
        }
        fn sync(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn session(&self) -> Option<HistorySessionId> {
            None
        }
    }

    fn refusing_history_engine() -> Reedline {
        let mut rl = seam_engine(Box::<Emacs>::default()).with_history(Box::new(RefusingHistory));
        rl.painter.force_prompt_anchored_for_test(0);
        rl
    }

    #[test]
    fn failed_history_save_still_returns_the_line_and_stashes_the_error() {
        let mut rl = refusing_history_engine();
        let signal = drive_until_signal(&mut rl, &[ch('l'), ch('s'), key(KeyCode::Enter)]);
        assert!(
            matches!(signal, Some(Signal::Success(ref s)) if s == "ls"),
            "got {signal:?}"
        );
        let err = rl.take_history_save_error();
        assert!(err.is_some(), "the save error is stashed");
        assert!(rl.take_history_save_error().is_none(), "cleared on read");
    }

    #[test]
    fn failed_history_save_keeps_the_entry_reachable() {
        let mut rl = refusing_history_engine();
        drive_until_signal(&mut rl, &[ch('l'), ch('s'), key(KeyCode::Enter)]);

        drive(&mut rl, &[key(KeyCode::Up)]);
        assert_eq!(rl.editor.get_buffer(), "ls", "Up recalls the unsaved entry");

        rl.update_last_command_context(&|mut item| {
            item.exit_status = Some(7);
            item
        })
        .expect("context update works off the store");
        assert_eq!(
            rl.history_excluded_item
                .as_ref()
                .and_then(|i| i.exit_status),
            Some(7)
        );
    }

    #[test]
    fn thread_safe() {
        fn f<S: Send>(_: S) {}
        f(Reedline::create());
    }

    #[test]
    fn thread_safe_with_idle_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        fn f<S: Send>(_: S) {}

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let reedline = Reedline::create()
            .with_poll_interval(Duration::from_millis(100))
            .with_idle_callback(Box::new(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }));

        // Verify that Reedline with idle_callback is still Send
        f(reedline);
    }

    #[test]
    fn idle_callback_builder_pattern() {
        // Test that with_idle_callback can be chained with other builder methods
        let _reedline = Reedline::create()
            .with_quick_completions(true)
            .with_poll_interval(Duration::from_millis(33))
            .with_idle_callback(Box::new(|| {}))
            .with_partial_completions(true);
    }

    #[test]
    fn mouse_click_moves_cursor_in_regular_mode() {
        let mut reedline = Reedline::create().with_mouse_click(MouseClickMode::Enabled);
        let prompt = DefaultPrompt::default();

        reedline
            .editor
            .set_buffer("hello".to_string(), UndoBehavior::CreateUndoPoint);
        reedline
            .editor
            .edit_buffer(|buf| buf.set_insertion_point(5), UndoBehavior::MoveCursor);

        reedline.last_render_snapshot = Some(RenderSnapshot {
            screen_width: 20,
            screen_height: 10,
            prompt_start_row: 0,
            prompt_height: 1,
            large_buffer: false,
            prompt_str_left: "".to_string(),
            prompt_indicator: "".to_string(),
            before_cursor: "hello".to_string(),
            after_cursor: "".to_string(),
            first_buffer_col: 0,
            menu_active: false,
            menu_start_row: None,
            large_buffer_extra_rows_after_prompt: None,
            large_buffer_offset: None,
            right_prompt: None,
        });

        let result = reedline.handle_event(
            &prompt,
            ReedlineEvent::Mouse {
                column: 0,
                row: 0,
                button: MouseButton::Left,
            },
        );

        assert!(matches!(result, Ok(EventStatus::Handled)));
        assert_eq!(reedline.current_insertion_point(), 0);
    }

    #[test]
    fn mouse_click_osc133_sets_semantic_markers() {
        let reedline = Reedline::create().with_mouse_click(MouseClickMode::EnabledWithOsc133);
        let markers = reedline
            .painter
            .semantic_markers()
            .expect("expected semantic markers");

        assert_eq!(
            markers.prompt_start(PromptKind::Primary).as_ref(),
            "\x1b]133;A;k=i;click_events=1\x1b\\"
        );
    }

    /// Drive one key per batch, stopping at the first `Signal`.
    fn drive_until_signal(rl: &mut Reedline, keys: &[KeyEvent]) -> Option<Signal> {
        let prompt = DefaultPrompt::default();
        for k in keys {
            match rl
                .process_input_batch(&prompt, vec![Event::Key(*k)])
                .expect("batch ok")
            {
                ControlFlow::Break(signal) => return Some(signal),
                ControlFlow::Continue(()) => {}
            }
        }
        None
    }

    /// `DefaultValidator` reads an unclosed `"` as incomplete, so `Enter` breaks
    /// the line instead of submitting it and leaves the buffer inspectable.
    fn helix_engine_with_validator() -> Reedline {
        let mut rl = Reedline::create()
            .with_edit_mode(Box::<crate::Helix>::default())
            .with_validator(Box::new(crate::DefaultValidator));
        rl.painter.force_prompt_anchored_for_test(0);
        rl
    }

    // --- submitting from helix normal mode ---
    //
    // The resting cursor is a selection and outlives the `next_mode` flip to
    // insert, so anything on the `Enter` path that opens with `delete_selection`
    // eats the covered grapheme. `InsertNewline` does, which makes the incomplete
    // branch the one that can observe it: a submitted buffer is cleared before
    // anything can be asserted about it.

    #[test]
    fn helix_normal_submit_keeps_the_grapheme_under_the_cursor() {
        let mut rl = helix_engine_with_validator();
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
            ],
        );
        assert!(signal.is_none(), "incomplete input must not submit");
        // Not `"ab\n`: the cursor rests *on* the `c`, which is not a selection
        // the break should consume.
        assert_eq!(rl.editor.get_buffer(), "\"abc\n");
    }

    #[test]
    fn helix_normal_submit_breaks_at_the_cursor_not_past_it() {
        let mut rl = helix_engine_with_validator();
        // `hh` walks the caret back onto the `a`.
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                ch('h'),
                ch('h'),
                key(KeyCode::Enter),
            ],
        );
        assert!(signal.is_none(), "incomplete input must not submit");
        // The head already sits past the covered `a`, so collapsing forward
        // breaks there. A vi-style `MoveRight` would step one further and give
        // `"ab\nc`; a plain deselect would land before it, at `"\nabc`.
        assert_eq!(rl.editor.get_buffer(), "\"a\nbc");
    }

    /// Helix rests *on* the line terminator under `BlockOverNewline`, which vi
    /// never does, so a break from there is a case vi's handling never answers.
    #[test]
    fn helix_normal_submit_breaks_from_a_terminator() {
        let mut rl = helix_engine_with_validator();
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
            ],
        );
        assert!(signal.is_none(), "incomplete input must not submit");
        assert_eq!(rl.editor.get_buffer(), "\"a\n\n");
    }

    /// A keybinding flip into insert must collapse the resting selection first,
    /// exactly as `i` does. The helix block cursor *is* a one-grapheme
    /// selection, and `insert_char` deletes the selection before inserting, so
    /// without the collapse the first keystroke replaces the covered grapheme.
    #[test]
    fn helix_change_mode_into_insert_keeps_the_covered_grapheme() {
        let mut bindings = crate::default_helix_normal_keybindings();
        bindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('z'),
            ReedlineEvent::HelixChangeMode("insert".into()),
        );
        let mut rl = Reedline::create()
            .with_edit_mode(Box::new(
                crate::Helix::default().with_normal_keybindings(bindings),
            ))
            .with_validator(Box::new(crate::DefaultValidator));
        rl.painter.force_prompt_anchored_for_test(0);
        // `hh` walks the caret back onto the `a`, so the cursor covers a
        // grapheme with buffer on both sides of it.
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                ch('h'),
                ch('h'),
                ch('z'),
                ch('X'),
            ],
        );
        assert!(signal.is_none(), "incomplete input must not submit");
        // Not `"Xbc`: the covered `a` is a resting cursor, not a selection the
        // insert should consume.
        assert_eq!(rl.editor.get_buffer(), "\"Xabc");
    }

    /// Vi visual rests min-width-1 under `RestPolicy::Block` just as helix does,
    /// so the same rule has to cover a `ViChangeMode` flip out of it. Without the
    /// collapse the visual selection is still live and the first keystroke
    /// replaces it.
    #[test]
    fn vi_change_mode_out_of_visual_keeps_the_covered_grapheme() {
        let mut bindings = crate::default_vi_normal_keybindings();
        bindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('z'),
            ReedlineEvent::ViChangeMode("insert".into()),
        );
        let mut rl = Reedline::create()
            .with_edit_mode(Box::new(crate::Vi::new(
                crate::default_vi_insert_keybindings(),
                bindings,
            )))
            .with_validator(Box::new(crate::DefaultValidator));
        rl.painter.force_prompt_anchored_for_test(0);
        // `hh` walks back onto the `a`, `v` enters visual covering it.
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                ch('h'),
                ch('h'),
                ch('v'),
                ch('z'),
                ch('X'),
            ],
        );
        assert!(signal.is_none(), "incomplete input must not submit");
        assert_eq!(rl.editor.get_buffer(), "\"Xabc");
    }

    /// The submitted path cannot assert on the buffer (`submit_buffer` clears
    /// it), so pin it through the returned signal instead.
    #[test]
    fn helix_normal_submit_returns_the_whole_buffer() {
        let mut rl = Reedline::create().with_edit_mode(Box::<crate::Helix>::default());
        rl.painter.force_prompt_anchored_for_test(0);
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
            ],
        );
        match signal {
            Some(Signal::Success(buffer)) => assert_eq!(buffer, "abc"),
            other => panic!("expected a submitted buffer, got {other:?}"),
        }
    }

    // --- `j` / `k` ---
    //
    // These lower to `ReedlineEvent::Up`/`Down` rather than a `MotionTarget`,
    // since which of line movement and history traversal applies is decided
    // against the whole buffer, above where a motion resolves.

    /// Two lines, built through the incomplete branch since a bare Enter would
    /// submit. Leaves the caret on the second line, in insert mode.
    fn two_line_helix_engine() -> Reedline {
        let mut rl = helix_engine_with_validator();
        drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
                ch('d'),
                ch('e'),
                ch('f'),
            ],
        );
        // `"abc` is 0..4, the terminator 4, `def` 5..8. Both lines are wide
        // enough for a column-preserving move to differ from a line-start one.
        assert_eq!(rl.editor.get_buffer(), "\"abc\ndef", "setup");
        rl
    }

    #[test]
    fn helix_normal_k_moves_a_line_before_it_reaches_history() {
        let mut rl = two_line_helix_engine();
        drive_until_signal(&mut rl, &[key(KeyCode::Esc), ch('k')]);
        assert!(
            rl.editor.insertion_point() < 4,
            "expected the caret on the first line, got {}",
            rl.editor.insertion_point()
        );
        assert_eq!(
            rl.editor.get_buffer(),
            "\"abc\ndef",
            "history must not load yet"
        );
    }

    #[test]
    fn helix_normal_k_recalls_history_at_the_first_line() {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        let signal = drive_until_signal(
            &mut rl,
            &[
                ch('o'),
                ch('n'),
                ch('e'),
                key(KeyCode::Esc),
                key(KeyCode::Enter),
            ],
        );
        assert!(
            matches!(signal, Some(Signal::Success(ref b)) if b == "one"),
            "setup: expected a submit, got {signal:?}"
        );
        // The buffer is empty now, so there is no line above to move to.
        drive_until_signal(&mut rl, &[key(KeyCode::Esc), ch('k')]);
        assert_eq!(rl.editor.get_buffer(), "one");
    }

    #[test]
    fn helix_select_extends_with_arrow_keys() {
        // The original report: `v` then arrows moved the caret but dropped the
        // anchor, since the arrows resolved through the mode-blind normal
        // table to `MoveRight { select: false }`.
        let mut rl = helix_engine_with_validator();
        drive_until_signal(
            &mut rl,
            &[
                ch('"'),
                ch('a'),
                ch('b'),
                ch('c'),
                key(KeyCode::Esc),
                ch('g'),
                ch('h'),
                ch('v'),
            ],
        );
        drive_until_signal(&mut rl, &[key(KeyCode::Right), key(KeyCode::Right)]);
        assert_eq!(
            rl.editor.get_selection(),
            Some((0, 3)),
            "arrows must extend like `l` does"
        );
    }

    #[test]
    fn helix_select_j_extends_to_the_column_normal_mode_would_land_on() {
        let mut rl = two_line_helix_engine();
        // `k` from the `f` (column 2) lands on the `b`, also column 2.
        drive_until_signal(&mut rl, &[key(KeyCode::Esc), ch('k')]);
        assert_eq!(rl.editor.insertion_point(), 2, "setup: expected the `b`");

        drive_until_signal(&mut rl, &[ch('v'), ch('j')]);
        assert_eq!(
            rl.editor.get_buffer(),
            "\"abc\ndef",
            "select mode must not traverse history"
        );
        let (start, end) = rl.editor.get_selection().expect("expected a selection");
        // Down from column 2 is the `f` at 7, not the line start at 5: the
        // extension has to stop where a normal-mode `j` would land.
        assert_eq!(
            (start, end),
            (2, 8),
            "expected the selection to reach the `f`, not the line start"
        );
    }

    #[test]
    fn helix_tilde_switches_case_and_keeps_the_selection() {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(&mut rl, &[ch('a'), ch('b'), key(KeyCode::Esc), ch('%')]);
        assert_eq!(rl.editor.get_selection(), Some((0, 2)), "setup");

        drive_until_signal(&mut rl, &[ch('~')]);
        assert_eq!(rl.editor.get_buffer(), "AB");
        // Still selected, so a second `~` acts on the same span.
        assert_eq!(rl.editor.get_selection(), Some((0, 2)));
        drive_until_signal(&mut rl, &[ch('~')]);
        assert_eq!(rl.editor.get_buffer(), "ab");
    }

    #[test]
    fn helix_backtick_lowercases_and_alt_backtick_uppercases() {
        let alt_backtick = KeyEvent::new(KeyCode::Char('`'), KeyModifiers::ALT);

        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(&mut rl, &[ch('A'), ch('b'), key(KeyCode::Esc), ch('%')]);
        assert_eq!(rl.editor.get_selection(), Some((0, 2)), "setup");

        drive_until_signal(&mut rl, &[ch('`')]);
        assert_eq!(rl.editor.get_buffer(), "ab");
        drive_until_signal(&mut rl, &[alt_backtick]);
        assert_eq!(rl.editor.get_buffer(), "AB");
        // Both keep the span, so they can be applied in sequence.
        assert_eq!(rl.editor.get_selection(), Some((0, 2)));
    }

    // --- `%`, `A`, `I` ---

    #[test]
    fn helix_percent_selects_the_whole_buffer() {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(
            &mut rl,
            &[ch('a'), ch('b'), ch('c'), key(KeyCode::Esc), ch('%')],
        );
        assert_eq!(rl.editor.get_selection(), Some((0, 3)));
    }

    /// `#1190`: `%` and `x` both leave a forward selection wider than one
    /// grapheme, and a backward extend from one used to move the caret two
    /// cells per press.
    #[rstest]
    #[case::select_all_h(ch('%'), ch('h'))]
    #[case::select_all_left(ch('%'), key(KeyCode::Left))]
    #[case::select_line_h(ch('x'), ch('h'))]
    #[case::select_line_left(ch('x'), key(KeyCode::Left))]
    fn helix_select_left_walks_one_cell_after_a_wide_selection(
        #[case] select: KeyEvent,
        #[case] left: KeyEvent,
    ) {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(
            &mut rl,
            &[
                ch('a'),
                ch('b'),
                ch('c'),
                ch('d'),
                key(KeyCode::Esc),
                select,
                ch('v'),
            ],
        );
        assert_eq!(rl.editor.get_selection(), Some((0, 4)));
        assert_eq!(
            rl.editor.insertion_point(),
            3,
            "caret rests on the final `d`"
        );
        for expected in [2, 1, 0] {
            drive_until_signal(&mut rl, &[left]);
            assert_eq!(
                rl.editor.insertion_point(),
                expected,
                "one cell per press, not two"
            );
        }
    }

    /// Appending has to land *past* the last grapheme: the block cursor rests on
    /// it, while insert mode sits between graphemes.
    #[test]
    fn helix_capital_a_appends_past_the_last_grapheme() {
        use crate::PromptHelixMode;

        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(
            &mut rl,
            &[ch(' '), ch('h'), ch('i'), key(KeyCode::Esc), ch('A')],
        );
        assert_eq!(rl.editor.insertion_point(), 3);
        assert!(matches!(
            rl.prompt_edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Insert)
        ));
        // Typing lands at the end rather than one grapheme short.
        drive_until_signal(&mut rl, &[ch('!')]);
        assert_eq!(rl.editor.get_buffer(), " hi!");
    }

    /// The leading space is what separates this from a plain line start.
    #[test]
    fn helix_capital_i_inserts_at_the_first_non_blank() {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        drive_until_signal(
            &mut rl,
            &[ch(' '), ch('h'), ch('i'), key(KeyCode::Esc), ch('I')],
        );
        assert_eq!(rl.editor.insertion_point(), 1);
        drive_until_signal(&mut rl, &[ch('!')]);
        assert_eq!(rl.editor.get_buffer(), " !hi");
    }

    #[test]
    fn with_edit_mode_builder_accepts_custom_helix_mode() {
        use crate::PromptHelixMode;

        let reedline = Reedline::create().with_edit_mode(Box::new(crate::Helix::default()));

        assert!(matches!(
            reedline.prompt_edit_mode(),
            PromptEditMode::Helix(PromptHelixMode::Insert)
        ));
    }

    #[test]
    fn break_signal_builder_pattern() {
        let signal = Arc::new(AtomicBool::new(false));
        let _reedline = Reedline::create()
            .with_quick_completions(true)
            .with_break_signal(signal)
            .with_partial_completions(true);
    }

    #[test]
    fn break_signal_is_send() {
        fn f<S: Send>(_: S) {}
        let signal = Arc::new(AtomicBool::new(false));
        f(Reedline::create().with_break_signal(signal));
    }

    #[test]
    fn take_repaint_request_is_false_without_a_handle() {
        let reedline = Reedline::create();
        assert!(!reedline.take_repaint_request());
        assert!(!reedline.take_repaint_request());
    }

    #[test]
    fn take_repaint_request_consumes_the_request() {
        let mut reedline = Reedline::create();
        let signal = reedline.repaint_signal();

        signal.request_repaint();
        assert!(reedline.take_repaint_request());
        // Consumed: no repaint left pending
        assert!(!reedline.take_repaint_request());

        // A new request is honored again
        signal.request_repaint();
        assert!(reedline.take_repaint_request());
        assert!(!reedline.take_repaint_request());
    }

    #[test]
    fn repaint_signal_switches_input_loop_to_polling() {
        let mut reedline = Reedline::create();
        assert!(
            !reedline.input_needs_polling(),
            "without external triggers the loop should block on input"
        );

        let _signal = reedline.repaint_signal();
        assert!(
            reedline.input_needs_polling(),
            "a repaint handle must switch the loop to polling so requests are noticed"
        );
    }

    #[test]
    fn repaint_request_during_active_read_survives_while_stale_ones_are_dropped() {
        // Emulates the loop's consumption pattern: read_line_helper drains any
        // stale pre-read_line request before painting the initial prompt, so
        // only requests raised afterwards trigger an extra repaint.
        let mut reedline = Reedline::create();
        let signal = reedline.repaint_signal();

        // Raised while no read_line is active -> dropped by the initial drain
        signal.request_repaint();
        reedline.take_repaint_request();
        assert!(!reedline.take_repaint_request());

        // Raised "mid-edit" -> observed by the next loop iteration
        signal.request_repaint();
        assert!(reedline.take_repaint_request());
    }

    #[test]
    fn repaint_signal_handles_share_one_flag() {
        // Every handle returned by `repaint_signal()` (and its clones) must
        // observe the same underlying flag, so a request from any of them is
        // seen exactly once by the loop.
        let mut reedline = Reedline::create();
        let a = reedline.repaint_signal();
        let b = reedline.repaint_signal();
        let c = a.clone();

        b.request_repaint();
        assert!(reedline.take_repaint_request());
        assert!(!reedline.take_repaint_request());

        // A request made through the clone is also observed.
        c.request_repaint();
        assert!(reedline.take_repaint_request());
    }

    #[test]
    fn repaint_signal_survives_behind_an_arc() {
        // This is what a shell would be using...
        // handed to a worker that knows nothing about `Reedline`.
        use std::sync::Arc;
        let mut reedline = Reedline::create();
        let shared: Arc<RepaintSignal> = Arc::new(reedline.repaint_signal());

        let worker = shared.clone();
        std::thread::spawn(move || worker.request_repaint())
            .join()
            .expect("worker thread panicked");

        assert!(reedline.take_repaint_request());
    }

    #[test]
    fn repaint_request_collapses_rapid_fire() {
        // Many requests arriving between two loop iterations must collapse into
        // a single repaint, not N. `take` is a swap(false), so only one take
        // should observe the request regardless of how many were raised.
        let mut reedline = Reedline::create();
        let signal = reedline.repaint_signal();

        for _ in 0..1_000 {
            signal.request_repaint();
        }
        assert!(reedline.take_repaint_request());
        assert!(!reedline.take_repaint_request());
    }

    #[test]
    fn repaint_signal_is_independent_of_break_signal() {
        // The two out-of-band triggers must not interfere: arming a repaint
        // request must not be mistaken for a break, and vice versa.
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        let mut reedline = Reedline::create().with_break_signal(Arc::new(AtomicBool::new(false)));
        let repaint = reedline.repaint_signal();

        repaint.request_repaint();
        assert!(reedline.take_repaint_request());
        assert!(
            !reedline
                .break_signal
                .as_ref()
                .unwrap()
                .load(std::sync::atomic::Ordering::Relaxed),
            "repaint must not toggle the break flag"
        );

        reedline
            .break_signal
            .as_ref()
            .unwrap()
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(
            reedline
                .break_signal
                .as_ref()
                .unwrap()
                .swap(false, std::sync::atomic::Ordering::Relaxed),
            "break flag must be independently observable"
        );
        assert!(
            !reedline.take_repaint_request(),
            "break must not leave a repaint pending"
        );
    }
    #[test]
    fn signal_external_break_pattern_match() {
        let buffer_content = "some partial input".to_string();
        let signal = Signal::ExternalBreak(buffer_content.clone());
        match signal {
            Signal::ExternalBreak(buf) => assert_eq!(buf, buffer_content),
            _ => panic!("Expected Signal::ExternalBreak"),
        }
    }

    fn reedline_with_abbrevs_and_string_lit_override(abbrevs: &[(&str, &str)]) -> Reedline {
        let map = abbrevs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Reedline::create()
            .with_highlighter(Box::new(ExampleHighlighter::default()))
            .with_abbreviations(map)
    }

    fn reedline_with_abbrevs_and_default_string_lit_check(abbrevs: &[(&str, &str)]) -> Reedline {
        let map = abbrevs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Reedline::create()
            .with_highlighter(Box::new(SimpleMatchHighlighter::default()))
            .with_abbreviations(map)
    }

    fn set_buffer_at_end(reedline: &mut Reedline, text: &str) {
        reedline.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString(text.to_string()),
        ]);
    }

    #[test]
    fn abbreviation_expands_on_submit() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "gc");
        let event = reedline.try_expand_abbreviation_at_cursor(true);
        assert!(event.is_some(), "expected expansion on submit");
        reedline.run_edit_commands(&match event.unwrap() {
            ReedlineEvent::Edit(cmds) => cmds,
            _ => panic!("expected Edit event"),
        });
        assert_eq!(reedline.current_buffer_contents(), "git commit");
    }

    #[test]
    fn abbreviation_expands_on_space_without_double_space() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        // When expansion is triggered by <space>, the triggering space has
        // already been inserted into the buffer before the expansion runs.
        set_buffer_at_end(&mut reedline, "gc ");
        let event = reedline.try_expand_abbreviation_at_cursor(false);
        assert!(event.is_some(), "expected expansion on space");
        reedline.run_edit_commands(&match event.unwrap() {
            ReedlineEvent::Edit(cmds) => cmds,
            _ => panic!("expected Edit event"),
        });
        // Exactly one trailing space: the triggering space must be replaced,
        // not left in place alongside the inserted suffix space.
        assert_eq!(reedline.current_buffer_contents(), "git commit ");
    }

    #[test]
    fn abbreviation_no_match_returns_none() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "gx");
        assert!(reedline.try_expand_abbreviation_at_cursor(true).is_none());
    }

    #[test]
    fn abbreviation_empty_buffer_returns_none() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        assert!(reedline.try_expand_abbreviation_at_cursor(true).is_none());
    }

    #[test]
    fn abbreviation_expands_last_word_only() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "sudo gc");
        let event = reedline.try_expand_abbreviation_at_cursor(true);
        assert!(event.is_some());
        reedline.run_edit_commands(&match event.unwrap() {
            ReedlineEvent::Edit(cmds) => cmds,
            _ => panic!("expected Edit event"),
        });
        assert_eq!(reedline.current_buffer_contents(), "sudo git commit");
    }

    #[rstest]
    #[case("\"hello gc", false)]
    #[case("'hello gc", false)]
    #[case("\"hello\" gc", true)]
    #[case("'Сегодня хороший gc", false)]
    #[case("'Сегодня' gc", true)]
    #[case("'今日はいい日だ gc", false)]
    #[case("'🔥🎉 gc", false)]
    fn abbreviation_string_detection_with_override(
        #[case] buffer: &str,
        #[case] should_expand: bool,
    ) {
        let mut reedline = reedline_with_abbrevs_and_string_lit_override(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, buffer);
        assert_eq!(
            reedline.try_expand_abbreviation_at_cursor(true).is_some(),
            should_expand
        );
    }

    #[rstest]
    #[case("\"hello gc")]
    #[case("'hello gc")]
    #[case("\"hello\" gc")]
    #[case("'Сегодня хороший gc")]
    #[case("'Сегодня' gc")]
    #[case("'今日はいい日だ gc")]
    #[case("'🔥🎉 gc")]
    fn abbreviation_string_detection_default(#[case] buffer: &str) {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, buffer);
        assert!(
            reedline.try_expand_abbreviation_at_cursor(true).is_some(),
            "must expand when highlighter does not override should_expand_abbr"
        );
    }

    #[test]
    fn abbreviation_non_ascii_key_and_expansion() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("café", "coffee shop")]);
        set_buffer_at_end(&mut reedline, "café");
        let event = reedline.try_expand_abbreviation_at_cursor(true);
        assert!(event.is_some(), "expected expansion for non-ASCII key");
        reedline.run_edit_commands(&match event.unwrap() {
            ReedlineEvent::Edit(cmds) => cmds,
            _ => panic!("expected Edit event"),
        });
        assert_eq!(reedline.current_buffer_contents(), "coffee shop");
    }

    #[test]
    fn try_expand_abbreviation_survives_multibyte_char_before_cursor() {
        // Regression: `word_end` was a raw byte subtraction of `offset` from the
        // byte cursor position. With a multi-byte char (e.g. pasted CJK) right
        // before the cursor, `word_end` could land inside that char, so slicing
        // the buffer panicked with "byte index N is not a char boundary".
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "中");
        // Place the cursor at byte offset 1, inside the 3-byte '中'. Pre-patch,
        // `&buffer[..1]` would panic here.
        let mut line_buffer = LineBuffer::new();
        line_buffer.set_buffer("中".to_string());
        line_buffer.set_insertion_point(1);
        reedline
            .editor
            .set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
        // Must return without panicking (no match, so `None` is expected).
        assert!(reedline.try_expand_abbreviation_at_cursor(true).is_none());
    }

    #[test]
    fn abbreviation_leading_spaces_returns_none() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "   ");
        assert!(reedline.try_expand_abbreviation_at_cursor(true).is_none());
    }

    #[test]
    fn abbreviation_mid_word_cursor_on_submit_returns_none() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "gcsomething");
        reedline.run_edit_commands(&[EditCommand::MoveToPosition {
            position: 2,
            select: false,
        }]);
        assert!(
            reedline.try_expand_abbreviation_at_cursor(true).is_none(),
            "must not expand the prefix of a word when the cursor is mid-word"
        );
    }

    #[test]
    fn abbreviation_expands_before_trailing_text_on_submit() {
        let mut reedline =
            reedline_with_abbrevs_and_default_string_lit_check(&[("gc", "git commit")]);
        set_buffer_at_end(&mut reedline, "gc rest");
        reedline.run_edit_commands(&[EditCommand::MoveToPosition {
            position: 2,
            select: false,
        }]);
        let event = reedline.try_expand_abbreviation_at_cursor(true);
        assert!(
            event.is_some(),
            "expected expansion at a real word boundary"
        );
        reedline.run_edit_commands(&match event.unwrap() {
            ReedlineEvent::Edit(cmds) => cmds,
            _ => panic!("expected Edit event"),
        });
        assert_eq!(reedline.current_buffer_contents(), "git commit rest");
    }

    // Feed one key as its own batch, mirroring real interactive input where each
    // keypress drives a separate `process_input_batch`.
    fn step_key(rl: &mut Reedline, k: KeyEvent) -> ControlFlow<Signal> {
        rl.process_input_batch(&DefaultPrompt::default(), vec![Event::Key(k)])
            .expect("batch ok")
    }

    #[test]
    fn abbreviation_expands_on_enter_in_vi_normal() {
        // Regression: a vi-normal block caret rests *on* the last grapheme, so
        // before the caret-release on Enter the submit-time scan saw `g` instead
        // of `gc` and silently skipped expansion.
        let mut abbreviations = HashMap::new();
        abbreviations.insert("gc".to_string(), "git commit".to_string());
        let mut rl = seam_engine(Box::<crate::Vi>::default()).with_abbreviations(abbreviations);

        let _ = step_key(&mut rl, ch('g'));
        let _ = step_key(&mut rl, ch('c'));
        let _ = step_key(&mut rl, key(KeyCode::Esc)); // vi normal, caret on 'c'
        match step_key(&mut rl, key(KeyCode::Enter)) {
            ControlFlow::Break(Signal::Success(buf)) => assert_eq!(buf, "git commit"),
            other => panic!("expected submit, got {other:?}"),
        }
    }

    #[test]
    fn vi_normal_enter_inserts_newline_at_end_not_mid_word() {
        // Regression: the same stranded block caret made an incomplete-input
        // newline land one grapheme short, splitting the last word (`ab` -> `a\nb`).
        struct AlwaysIncomplete;
        impl crate::Validator for AlwaysIncomplete {
            fn validate(&self, _line: &str) -> ValidationResult {
                ValidationResult::Incomplete
            }
        }
        let mut rl =
            seam_engine(Box::<crate::Vi>::default()).with_validator(Box::new(AlwaysIncomplete));

        let _ = step_key(&mut rl, ch('a'));
        let _ = step_key(&mut rl, ch('b'));
        let _ = step_key(&mut rl, key(KeyCode::Esc)); // vi normal, caret on 'b'
        let _ = step_key(&mut rl, key(KeyCode::Enter)); // incomplete -> insert newline
        assert_eq!(rl.editor.get_buffer(), "ab\n");
    }

    #[cfg(feature = "bashisms")]
    fn reedline_with_history_and_string_lit_check(entries: &[&str]) -> Reedline {
        let mut reedline =
            Reedline::create().with_highlighter(Box::new(ExampleHighlighter::default()));
        for entry in entries {
            reedline
                .history
                .save(HistoryItem::from_command_line(*entry))
                .expect("failed to save history");
        }
        reedline
    }

    #[cfg(feature = "bashisms")]
    fn reedline_with_history_default(entries: &[&str]) -> Reedline {
        let mut reedline =
            Reedline::create().with_highlighter(Box::new(SimpleMatchHighlighter::default()));
        for entry in entries {
            reedline
                .history
                .save(HistoryItem::from_command_line(*entry))
                .expect("failed to save history");
        }
        reedline
    }

    #[rstest]
    #[case("!!", true)]
    #[case("\"echo !!", false)]
    #[case("'echo !!", false)]
    #[case("'echo' !!", true)]
    #[case("\"echo !git", false)]
    #[case("'echo !git", false)]
    #[case("'Сегодня !!", false)]
    #[case("'今日は !!", false)]
    #[case("'🔥 !!", false)]
    #[cfg(feature = "bashisms")]
    fn bang_string_detection_with_override(#[case] buffer: &str, #[case] should_expand: bool) {
        let mut reedline = reedline_with_history_and_string_lit_check(&["git status"]);
        set_buffer_at_end(&mut reedline, buffer);
        assert_eq!(reedline.parse_bang_command().is_some(), should_expand);
    }

    #[rstest]
    #[case("\"echo !!")]
    #[case("'echo !!")]
    #[case("'echo' !!")]
    #[case("\"echo !git")]
    #[case("'echo !git")]
    #[case("'Сегодня !!")]
    #[case("'今日は !!")]
    #[case("'🔥 !!")]
    #[cfg(feature = "bashisms")]
    fn bang_always_expands_without_override(#[case] buffer: &str) {
        let mut reedline = reedline_with_history_default(&["git status"]);
        set_buffer_at_end(&mut reedline, buffer);
        assert!(
            reedline.parse_bang_command().is_some(),
            "must expand when highlighter does not override should_expand_abbr"
        );
    }

    #[rstest]
    #[case("")]
    #[case("line of text")]
    #[case(
        "longgggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg line of text"
    )]
    fn test_move_to_line_start(#[case] input: &str) {
        let mut reedline = Reedline::create();

        // Write the string, and then move to the start of the line.
        let insertion = EditCommand::InsertString(String::from(input));
        reedline.run_edit_commands(&[insertion]);

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(reedline.editor.line_buffer().insertion_point(), 0);
    }

    #[rstest]
    #[case("")]
    #[case("line of text")]
    #[case(
        "longgggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg line of text"
    )]
    fn test_move_to_line_start_history(#[case] input: &str) {
        let mut reedline = Reedline::create();

        // Enter the string into history, then scroll back up and move to the start of the line.
        let history = HistoryItem::from_command_line(input);
        reedline.history.save(history).unwrap();

        reedline.previous_history().expect("history ok");

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(reedline.editor.line_buffer().insertion_point(), 0);
    }

    #[rstest]
    #[case("a\nb", 2)]
    #[case("123456789\n123456789\n123456789", 20)]
    #[case("0\n1\n2\n3\n4\n5\n6\n7\n8\n9", 18)]
    fn test_move_to_line_start_multiline(#[case] input: &str, #[case] last_line_start: usize) {
        let mut reedline = Reedline::create();

        // Write the string, and then move to the start of the last line.
        let insertion = EditCommand::InsertString(String::from(input));
        reedline.run_edit_commands(&[insertion]);

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(
            reedline.editor.line_buffer().insertion_point(),
            last_line_start
        );
    }

    #[rstest]
    #[case("a\nb")]
    #[case("123456789\n123456789\n123456789")]
    #[case("0\n1\n2\n3\n4\n5\n6\n7\n8\n9")]
    fn test_move_to_line_start_multiline_history_up_start(#[case] input: &str) {
        let mut reedline = Reedline::create();

        // Enter the string into history, then scroll back up and move to the start of the line.
        let history = HistoryItem::from_command_line(input);
        reedline.history.save(history).unwrap();

        reedline.previous_history().expect("history ok");

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(reedline.editor.line_buffer().insertion_point(), 0);
    }

    #[rstest]
    #[case("a\nb", 2)]
    #[case("123456789\n123456789\n123456789", 10)]
    #[case("0\n1\n2\n3\n4\n5\n6\n7\n8\n9", 2)]
    fn test_move_to_line_start_multiline_history_up_down_start(
        #[case] input: &str,
        #[case] second_line_start: usize,
    ) {
        let mut reedline = Reedline::create();

        // Enter the string again, then scroll up in history, move down one line,
        // and move to the start of the second line.
        let history = HistoryItem::from_command_line(input);
        reedline.history.save(history).unwrap();

        reedline.previous_history().expect("history ok");

        reedline.down_command().expect("history ok");

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(
            reedline.editor.line_buffer().insertion_point(),
            second_line_start
        );
    }

    #[rstest]
    #[case("a\nb", 2)]
    #[case("123456789\n123456789\n123456789", 20)]
    #[case("0\n1\n2\n3\n4\n5\n6\n7\n8\n9", 18)]
    fn test_move_to_line_start_multiline_history_up_end_start(
        #[case] input: &str,
        #[case] last_line_start: usize,
    ) {
        let mut reedline = Reedline::create();

        // Enter the string again, then scroll up in history, move to the end of the text,
        // and move to the start of the last line.
        let history = HistoryItem::from_command_line(input);
        reedline.history.save(history).unwrap();

        reedline.previous_history().expect("history ok");

        let move_to_end = EditCommand::MoveToEnd { select: false };
        reedline.run_edit_commands(&[move_to_end]);

        let move_to_start = EditCommand::MoveToLineStart { select: false };
        reedline.run_edit_commands(&[move_to_start]);

        assert_eq!(
            reedline.editor.line_buffer().insertion_point(),
            last_line_start
        );
    }

    #[test]
    fn test_complete_line_from_history() {
        let completer = Box::new(DefaultCompleter::new(Vec::from([String::from("67")])));
        let completion_menu = ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        ));
        let mut reedline = Reedline::create()
            .with_quick_completions(true)
            .with_completer(completer)
            .with_menu(completion_menu);

        // Save "6" to the history and scroll back to it
        let history = HistoryItem::from_command_line("6");
        reedline.history.save(history).unwrap();
        reedline.previous_history().expect("history ok");
        assert_eq!(reedline.current_buffer_contents(), "6");

        // Perform quick completion
        let prompt = DefaultPrompt::default();
        let completion = ReedlineEvent::Menu(String::from("completion_menu"));
        reedline.handle_event(&prompt, completion).unwrap();
        assert_eq!(reedline.current_buffer_contents(), "67");

        // Insert the "x" to the prompt
        let insertion = EditCommand::InsertString(String::from("x"));
        reedline.run_edit_commands(&[insertion]);
        assert_eq!(reedline.current_buffer_contents(), "67x");
    }

    /// A completer that computes in the background: the first request cannot be answered
    /// for the line on screen, and only later ones carry its values. This is what a cold
    /// cache looks like from the engine's side (nushell/reedline#1142).
    struct DeferredCompleter {
        first_answer: CompletionResult,
        values: Vec<String>,
        dispatched: bool,
    }

    impl DeferredCompleter {
        /// A cold cache: nothing to show at all until the background work lands.
        fn pending(values: &[&str]) -> Self {
            Self::new(CompletionResult::Pending, values)
        }

        /// A warm-but-wrong cache: `stale` is answered from a neighbouring entry, so the
        /// value is real but its span belongs to `origin_buffer` rather than the line.
        fn stale(stale: &str, origin_buffer: &str, values: &[&str]) -> Self {
            let first_answer = CompletionResult::Stale {
                suggestions: vec![Suggestion {
                    value: stale.to_string(),
                    span: Span {
                        start: 0,
                        end: origin_buffer.len(),
                    },
                    ..Default::default()
                }]
                .into(),
                origin: CompletionOrigin::new(origin_buffer, origin_buffer.len()),
                partial: None,
            };
            Self::new(first_answer, values)
        }

        fn new(first_answer: CompletionResult, values: &[&str]) -> Self {
            Self {
                first_answer,
                values: values.iter().map(|value| value.to_string()).collect(),
                dispatched: false,
            }
        }
    }

    impl Completer for DeferredCompleter {
        fn complete(&mut self, _line: &str, pos: usize) -> CompletionResult {
            if !self.dispatched {
                self.dispatched = true;
                return self.first_answer.clone();
            }
            CompletionResult::fresh(
                self.values
                    .iter()
                    .map(|value| Suggestion {
                        value: value.clone(),
                        span: Span { start: 0, end: pos },
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
            )
        }

        fn poll_completion(&mut self) -> CompletionStatus {
            if self.dispatched {
                CompletionStatus::Ready
            } else {
                CompletionStatus::Idle
            }
        }
    }

    /// [`engine_awaiting`] with partial completions off.
    fn engine_awaiting_completions(values: &[&str], buffer: &str, quick: bool) -> Reedline {
        engine_awaiting(values, buffer, quick, false)
    }

    /// Engine with `quick` and `partial` completions and a background completer over
    /// `values`, with `buffer` typed and the completion menu activated — the state right
    /// after Tab, while the completer is still working.
    fn engine_awaiting(values: &[&str], buffer: &str, quick: bool, partial: bool) -> Reedline {
        let (reedline, _) = activate_menu_over(
            Box::new(DeferredCompleter::pending(values)),
            buffer,
            quick,
            partial,
        );

        // Nothing could be decided yet, the premise of every test below.
        assert_eq!(reedline.current_buffer_contents(), buffer);
        reedline
    }

    /// Type `buffer` and press Tab, against a completer that answers however the test
    /// needs it to. Returns the engine and how the menu activation was handled.
    fn activate_menu_over(
        completer: Box<dyn Completer + Send>,
        buffer: &str,
        quick: bool,
        partial: bool,
    ) -> (Reedline, EventStatus) {
        let completion_menu = ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        ));
        let mut reedline = Reedline::create()
            .with_completer(completer)
            .with_menu(completion_menu)
            .with_quick_completions(quick)
            .with_partial_completions(partial);

        // Settling repaints, which needs a painter that believes it is on a terminal.
        // Size first: `handle_resize` invalidates the anchor the next line pins.
        reedline.painter.handle_resize(80, 24);
        reedline.painter.force_prompt_anchored_for_test(0);

        reedline.run_edit_commands(&[EditCommand::InsertString(buffer.to_string())]);
        let status = reedline
            .handle_event(
                &DefaultPrompt::default(),
                ReedlineEvent::Menu(String::from("completion_menu")),
            )
            .unwrap();

        (reedline, status)
    }

    fn settle(reedline: &mut Reedline) {
        reedline
            .settle_completions(&DefaultPrompt::default())
            .unwrap();
    }

    /// The regression: a lone suggestion that arrives after the menu opened must still be
    /// accepted, exactly as a synchronous completer's would have been.
    #[test]
    fn quick_completion_accepts_a_lone_suggestion_that_arrives_late() {
        let mut reedline = engine_awaiting_completions(&["crates"], "cr", true);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "crates");
        assert!(!menu_is_active(&reedline), "accepting closes the menu");
    }

    /// The decision belongs to the keystroke that asked for it. Once the user has typed
    /// on, a late result must not rewrite the line underneath them.
    #[test]
    fn a_late_lone_suggestion_is_dropped_once_the_line_moved_on() {
        let mut reedline = engine_awaiting_completions(&["crates"], "cr", true);

        send_edit(&mut reedline, EditCommand::InsertChar('a'));
        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "cra");
    }

    /// Arming happens before the count is final, so settling has to re-check it: several
    /// suggestions mean a menu, not an acceptance.
    #[test]
    fn late_results_with_several_suggestions_only_populate_the_menu() {
        let mut reedline = engine_awaiting_completions(&["test", "this", "that"], "t", true);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "t");
        assert!(menu_is_active(&reedline));
    }

    /// An arm is spent by the result that settles it, so a later request cannot inherit it.
    #[test]
    fn an_unsatisfied_arm_does_not_fire_on_a_later_result() {
        let mut reedline = engine_awaiting_completions(&["test", "this", "that"], "t", true);

        settle(&mut reedline);
        assert!(reedline.deferred_menu_completion.is_none(), "arm is spent");

        // A second round of results, now down to one value, with no Tab in between.
        reedline.completer = Box::new(DeferredCompleter::pending(&["test"]));
        reedline.completer.complete("t", 1);
        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "t");
    }

    /// The other half of the same Tab: partial completions read the same empty menu the
    /// quick check did, so a shared prefix must be spliced in when the values land.
    #[test]
    fn late_results_splice_the_shared_prefix() {
        let mut reedline = engine_awaiting(&["nu-cmd-base", "nu-cmd-lang"], "nu-cm", true, true);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "nu-cmd-");
    }

    /// A shared prefix is spliced under `partial` alone, with no quick completions.
    #[test]
    fn late_results_splice_the_shared_prefix_without_quick_completions() {
        let mut reedline = engine_awaiting(&["nu-cmd-base", "nu-cmd-lang"], "nu-cm", false, true);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "nu-cmd-");
    }

    /// Accepting a lone value wins over splicing, as it does on activation.
    #[test]
    fn a_lone_late_suggestion_is_accepted_rather_than_spliced() {
        let mut reedline = engine_awaiting(&["crates"], "cr", true, true);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "crates");
        assert!(!menu_is_active(&reedline));
    }

    /// Splicing is owed to a keystroke too, and is void once the line moved on.
    #[test]
    fn a_late_shared_prefix_is_dropped_once_the_line_moved_on() {
        let mut reedline = engine_awaiting(&["nu-cmd-base", "nu-cmd-lang"], "nu-cm", true, true);

        send_edit(&mut reedline, EditCommand::InsertChar('d'));
        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "nu-cmd");
    }

    /// With quick completions off, late results populate the menu and nothing else.
    #[test]
    fn late_results_never_auto_accept_without_quick_completions() {
        let mut reedline = engine_awaiting_completions(&["crates"], "cr", false);

        settle(&mut reedline);

        assert_eq!(reedline.current_buffer_contents(), "cr");
        assert!(menu_is_active(&reedline));
    }

    /// A lone *stale* suggestion looks like a lone fresh one to the quick completion
    /// check, but accepting it is a no-op. The Tab must still be honoured once the real
    /// answer lands.
    #[test]
    fn a_lone_stale_suggestion_does_not_swallow_the_completion() {
        let (mut reedline, _) = activate_menu_over(
            Box::new(DeferredCompleter::stale("console", "co", &["crates"])),
            "cr",
            true,
            false,
        );

        // The stale span is refused, so the buffer is untouched.
        assert_eq!(reedline.current_buffer_contents(), "cr");

        settle(&mut reedline);

        assert_eq!(
            reedline.current_buffer_contents(),
            "crates",
            "the fresh result was dropped because the stale one closed the menu first"
        );
    }

    /// The same lone stale value through the second route: `MenuNext` with the menu
    /// already open. The accept is refused downstream either way, but the `Enter` it
    /// rode on would still deactivate the menu, so the pending completion had nothing
    /// left to land in.
    #[test]
    fn menu_next_does_not_close_the_menu_over_a_lone_stale_value() {
        let (mut reedline, _) = activate_menu_over(
            Box::new(DeferredCompleter::stale("console", "co", &["crates"])),
            "cr",
            true,
            false,
        );
        assert!(menu_is_active(&reedline), "setup");

        reedline
            .handle_event(&DefaultPrompt::default(), ReedlineEvent::MenuNext)
            .unwrap();

        assert!(
            menu_is_active(&reedline),
            "MenuNext accepted a provisional lone value and closed the menu"
        );
        assert_eq!(reedline.current_buffer_contents(), "cr");

        // With the menu still open, the arm from the activation is still owed.
        settle(&mut reedline);
        assert_eq!(reedline.current_buffer_contents(), "crates");
    }

    /// The flicker: a menu opened over an answer that is not about this line stays off
    /// screen, so a Tab that resolves to one suggestion never draws a menu at all.
    #[test]
    fn an_opening_menu_is_not_visible_until_answered() {
        let (reedline, _) = activate_menu_over(
            Box::new(DeferredCompleter::stale("console", "co", &["crates"])),
            "cr",
            true,
            false,
        );

        let menu = reedline
            .menus
            .iter()
            .find(|menu| menu.is_active())
            .expect("the menu is open");
        assert!(
            menu.is_awaiting_first_answer(),
            "it would only be taken away again once the real answer lands"
        );
    }

    fn menu_is_active(reedline: &Reedline) -> bool {
        reedline.menus.iter().any(|menu| menu.is_active())
    }

    /// Engine with a completion menu activated on a "th" buffer. "th" matches
    /// two words, so quick completions don't auto-select on activation.
    fn engine_with_active_menu(quick: bool, persistent: bool) -> Reedline {
        let completer = Box::new(DefaultCompleter::new_with_wordlen(
            vec![
                String::from("test"),
                String::from("this"),
                String::from("that"),
            ],
            1,
        ));
        let completion_menu = ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        ));
        let mut reedline = Reedline::create()
            .with_completer(completer)
            .with_menu(completion_menu)
            .with_quick_completions(quick)
            .with_persistent_menus(persistent);

        reedline.run_edit_commands(&[EditCommand::InsertString(String::from("th"))]);
        reedline
            .handle_event(
                &DefaultPrompt::default(),
                ReedlineEvent::Menu(String::from("completion_menu")),
            )
            .unwrap();
        assert!(menu_is_active(&reedline));
        reedline
    }

    /// Engine with a completion menu open over "th" and partial completions on, so
    /// `MenuNext` reaches the completer through `can_partially_complete`.
    fn engine_with_partial_completion_menu() -> Reedline {
        let completer = Box::new(DefaultCompleter::new_with_wordlen(
            vec![String::from("this"), String::from("that")],
            1,
        ));
        let completion_menu = ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        ));
        let mut reedline = Reedline::create()
            .with_completer(completer)
            .with_menu(completion_menu)
            .with_partial_completions(true);

        reedline.run_edit_commands(&[EditCommand::InsertString(String::from("th"))]);
        reedline
            .handle_event(
                &DefaultPrompt::default(),
                ReedlineEvent::Menu(String::from("completion_menu")),
            )
            .unwrap();
        assert!(menu_is_active(&reedline));
        reedline
    }

    /// The anchor cache exists to keep `cursor::position()` off the hot path (#1090),
    /// so an event may only spend that round-trip when it actually runs a host
    /// completer. Which events those are does not follow from whether the menu's
    /// selection moved: `MenuNext` splices a partial completion first, `MenuPrevious`
    /// does not. See #1130.
    #[rstest]
    #[case::next_queries_the_completer(ReedlineEvent::MenuNext, false)]
    #[case::previous_only_moves(ReedlineEvent::MenuPrevious, true)]
    fn menu_events_invalidate_the_anchor_only_when_they_query(
        #[case] event: ReedlineEvent,
        #[case] stays_verified: bool,
    ) {
        let mut reedline = engine_with_partial_completion_menu();
        reedline.painter.force_prompt_anchored_for_test(0);

        reedline
            .handle_event(&DefaultPrompt::default(), event)
            .unwrap();

        assert_eq!(
            reedline.painter.prompt_anchor_is_verified_for_test(),
            stays_verified
        );
    }

    fn send_edit(reedline: &mut Reedline, command: EditCommand) {
        reedline
            .handle_event(
                &DefaultPrompt::default(),
                ReedlineEvent::Edit(vec![command]),
            )
            .unwrap();
    }

    #[rstest]
    #[case(false, false)]
    #[case(false, true)]
    #[case(true, false)]
    #[case(true, true)]
    fn test_menu_persistence_while_erasing(#[case] quick: bool, #[case] persistent: bool) {
        let mut reedline = engine_with_active_menu(quick, persistent);

        // quick completions close the menu on any backspace unless menus are persistent
        send_edit(&mut reedline, EditCommand::Backspace);
        assert_eq!(reedline.current_buffer_contents(), "t");
        assert_eq!(menu_is_active(&reedline), persistent || !quick);

        // emptying the buffer closes the menu unless menus are persistent
        send_edit(&mut reedline, EditCommand::Backspace);
        assert!(reedline.current_buffer_contents().is_empty());
        assert_eq!(menu_is_active(&reedline), persistent);
    }

    #[rstest]
    #[case(EditCommand::BackspaceWord)]
    #[case(EditCommand::MoveToLineStart { select: false })]
    fn test_menu_persistence_covers_all_quick_dismissal_commands(#[case] command: EditCommand) {
        for persistent in [false, true] {
            let mut reedline = engine_with_active_menu(true, persistent);
            send_edit(&mut reedline, command.clone());
            assert_eq!(menu_is_active(&reedline), persistent);
        }
    }

    fn send(reedline: &mut Reedline, event: ReedlineEvent) -> EventStatus {
        reedline
            .handle_event(&DefaultPrompt::default(), event)
            .unwrap()
    }

    /// Engine with a completion menu opened over "th" by a completer that has no
    /// word starting with it, so the menu is active but holds no values.
    fn engine_with_empty_menu() -> Reedline {
        let completer = Box::new(DefaultCompleter::new_with_wordlen(
            vec![String::from("xylophone")],
            1,
        ));
        let completion_menu = ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        ));
        let mut reedline = Reedline::create()
            .with_completer(completer)
            .with_menu(completion_menu)
            .with_quick_completions(true);

        reedline.run_edit_commands(&[EditCommand::InsertString(String::from("th"))]);
        send(
            &mut reedline,
            ReedlineEvent::Menu(String::from("completion_menu")),
        );
        assert!(menu_is_active(&reedline), "setup");
        assert!(reedline.menus[0].get_values().is_empty(), "setup");
        reedline
    }

    #[test]
    fn menu_accept_splices_the_selection_and_closes_the_menu() {
        let mut reedline = engine_with_active_menu(true, false);

        let status = send(&mut reedline, ReedlineEvent::MenuAccept);

        assert!(matches!(status, EventStatus::Handled));
        assert_eq!(reedline.current_buffer_contents(), "that");
        assert!(!menu_is_active(&reedline));
    }

    #[test]
    fn menu_accept_without_an_open_menu_is_inapplicable() {
        let mut reedline = Reedline::create();
        reedline.run_edit_commands(&[EditCommand::InsertString(String::from("th"))]);

        let status = send(&mut reedline, ReedlineEvent::MenuAccept);

        assert!(matches!(status, EventStatus::Inapplicable));
        assert_eq!(reedline.current_buffer_contents(), "th");
    }

    /// Nothing to splice, so the keypress is not spent on closing the menu; the
    /// event falls through to whatever the binding lists next.
    #[test]
    fn menu_accept_over_an_empty_menu_is_inapplicable_and_leaves_it_open() {
        let mut reedline = engine_with_empty_menu();

        let status = send(&mut reedline, ReedlineEvent::MenuAccept);

        assert!(matches!(status, EventStatus::Inapplicable));
        assert!(menu_is_active(&reedline));
        assert_eq!(reedline.current_buffer_contents(), "th");
    }

    /// The binding this exists for: space accepts the highlighted completion and
    /// then types itself, and stays a plain space when no menu is open.
    #[test]
    fn menu_accept_chains_with_an_edit_as_a_space_binding() {
        let space_binding = || {
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::MenuAccept,
                ReedlineEvent::Edit(vec![EditCommand::InsertChar(' ')]),
            ])
        };

        let mut reedline = engine_with_active_menu(true, false);
        send(&mut reedline, space_binding());
        assert_eq!(reedline.current_buffer_contents(), "that ");
        assert!(!menu_is_active(&reedline));

        let mut reedline = Reedline::create();
        reedline.run_edit_commands(&[EditCommand::InsertString(String::from("th"))]);
        send(&mut reedline, space_binding());
        assert_eq!(reedline.current_buffer_contents(), "th ");
    }

    /// A hinter that always offers a fixed suggestion, so the completion flow can
    /// be driven without the paint cycle that normally refreshes the hint.
    struct FixedHinter(&'static str);
    impl Hinter for FixedHinter {
        fn handle(&mut self, _: &str, _: usize, _: &dyn History, _: bool, _: &str) -> String {
            self.0.to_string()
        }
        fn complete_hint(&self) -> String {
            self.0.to_string()
        }
        fn next_hint_token(&self) -> String {
            self.0.to_string()
        }
    }

    fn vi_with_hint(hint: &'static str) -> Reedline {
        seam_engine(Box::<crate::Vi>::default()).with_hinter(Box::new(FixedHinter(hint)))
    }

    fn helix_with_hint(hint: &'static str) -> Reedline {
        seam_engine(Box::<crate::Helix>::default()).with_hinter(Box::new(FixedHinter(hint)))
    }

    #[test]
    fn vi_normal_history_hint_appends_at_buffer_end() {
        // The reported bug: a block caret rests on the last grapheme, so the
        // completion must append *after* it, not split it.
        let mut rl = vi_with_hint("def");
        rl.run_edit_commands(&[EditCommand::InsertString("abc".into())]);
        drive(&mut rl, &[key(KeyCode::Esc)]); // vi normal, caret on 'c' at the end
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abcdef");
    }

    #[test]
    fn vi_visual_selection_blocks_hint_completion() {
        // A hint completing over a selection would run through `delete_selection`
        // and clobber it — the empty-cursor guard must suppress it.
        let mut rl = vi_with_hint("def");
        rl.run_edit_commands(&[EditCommand::InsertString("abc".into())]);
        drive(&mut rl, &[key(KeyCode::Esc), ch('v')]); // visual: selection covers 'c' to len
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abc");
    }

    /// Helix normal rests as a min-width-1 block, not an empty point. The
    /// buffer-end guard must read that as a resting caret, not a selection, or
    /// a history hint never completes in normal mode.
    #[test]
    fn helix_normal_history_hint_appends_at_buffer_end() {
        let mut rl = helix_with_hint("def");
        type_each(&mut rl, &[ch('a'), ch('b'), ch('c'), key(KeyCode::Esc)]);
        assert!(
            !rl.editor.line_buffer().cursor().is_empty(),
            "setup: the helix block caret is a one-grapheme range"
        );
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abcdef");
    }

    /// A `v`-started helix selection is still protected, like vi visual.
    #[test]
    fn helix_select_selection_blocks_hint_completion() {
        let mut rl = helix_with_hint("def");
        type_each(
            &mut rl,
            &[ch('a'), ch('b'), ch('c'), key(KeyCode::Esc), ch('v')],
        );
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abc");
    }

    /// A retained motion selection in helix *normal* (here `b` sweeping back
    /// over the word) is multi-grapheme, so it blocks completion even though
    /// normal is not a selection mode.
    #[test]
    fn helix_normal_motion_selection_blocks_hint_completion() {
        let mut rl = helix_with_hint("def");
        type_each(
            &mut rl,
            &[ch('a'), ch('b'), ch('c'), key(KeyCode::Esc), ch('b')],
        );
        let cursor = rl.editor.line_buffer().cursor();
        assert!(
            cursor.end() - cursor.start() > 1,
            "setup: `b` leaves a multi-grapheme selection standing"
        );
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abc");
    }

    /// The issue as filed pressed a key, not an event: `Right` reaches
    /// `HistoryHintComplete` through the keymap's `UntilFound` chain, so this
    /// covers the wiring the event-level tests above skip past.
    #[test]
    fn helix_normal_right_key_completes_the_hint() {
        let mut rl = helix_with_hint("def");
        type_each(&mut rl, &[ch('a'), ch('b'), ch('c'), key(KeyCode::Esc)]);
        drive(&mut rl, &[key(KeyCode::Right)]);
        assert_eq!(rl.editor.get_buffer(), "abcdef");
    }

    /// The prefix-search side of the same guard: helix normal's resting block
    /// still counts as "at buffer end", so `k` prefix-searches history like vi
    /// normal instead of walking it plainly.
    #[test]
    fn helix_normal_k_uses_prefix_search() {
        let mut rl = seam_engine(Box::<crate::Helix>::default());
        for entry in ["ls -la", "ls /tmp", "echo hi"] {
            rl.history
                .save(HistoryItem::from_command_line(entry))
                .unwrap();
        }
        type_each(&mut rl, &[ch('l'), ch('s'), key(KeyCode::Esc)]);
        drive(&mut rl, &[ch('k')]);
        assert_eq!(rl.editor.get_buffer(), "ls /tmp");
    }

    #[test]
    fn undo_removes_accepted_history_hint() {
        let mut rl = vi_with_hint("def");
        rl.run_edit_commands(&[EditCommand::InsertString("abc".into())]);
        drive(&mut rl, &[key(KeyCode::Esc)]);
        rl.handle_event(
            &DefaultPrompt::default(),
            ReedlineEvent::HistoryHintComplete,
        )
        .unwrap();
        assert_eq!(rl.editor.get_buffer(), "abcdef");
        rl.run_edit_commands(&[EditCommand::Undo]);
        assert_eq!(rl.editor.get_buffer(), "abc");
    }

    #[test]
    fn vi_normal_down_rests_on_last_grapheme() {
        // Down onto a shorter last line must rest *on* the last grapheme, not the
        // gap past it: `down_command` has to settle under the `OnGrapheme` policy.
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        rl.run_edit_commands(&[EditCommand::InsertString("abc\nd".into())]); // a0 b1 c2 \n3 d4
        drive(&mut rl, &[key(KeyCode::Esc)]); // vi normal
        rl.run_edit_commands(&[
            EditCommand::MoveToStart { select: false },
            EditCommand::MoveRight { select: false },
            EditCommand::MoveRight { select: false },
        ]); // caret on 'c' (col 2 of line 1)
        rl.down_command().expect("history ok");
        assert_eq!(rl.editor.insertion_point(), 4); // on 'd', not 5 (past it)
    }

    #[test]
    fn vi_normal_to_end_rests_on_last_grapheme() {
        // `Alt+>` (ToEnd) is bound in vi normal; it must land on the last char.
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        rl.run_edit_commands(&[EditCommand::InsertString("abc".into())]);
        drive(&mut rl, &[key(KeyCode::Esc)]);
        rl.run_edit_commands(&[EditCommand::MoveToStart { select: false }]); // 'a'
        drive(
            &mut rl,
            &[KeyEvent::new(KeyCode::Char('>'), KeyModifiers::ALT)],
        );
        assert_eq!(rl.editor.insertion_point(), 2); // 'c', not 3 (past it)
    }

    #[test]
    fn vi_normal_single_line_down_rests_on_last_grapheme() {
        // Single-line buffer: `down` hits the last line and routes to history nav,
        // which positions the cursor outside the command path. It must still
        // settle on the last grapheme, not the gap past it.
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        rl.run_edit_commands(&[EditCommand::InsertString("abc".into())]);
        drive(&mut rl, &[key(KeyCode::Esc)]); // vi normal, on 'c'
        rl.down_command().expect("history ok"); // last line -> next_history (no forward entry -> draft)
        assert_eq!(rl.editor.insertion_point(), 2); // 'c', not 3 (past it)
    }

    #[test]
    fn vi_normal_end_of_line_rests_on_last_grapheme() {
        // `$` on an interior line lands ON the last char, not the gap before `\n`.
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        rl.run_edit_commands(&[EditCommand::InsertString("abc\ndef".into())]); // a0 b1 c2 \n3
        drive(&mut rl, &[key(KeyCode::Esc)]);
        rl.run_edit_commands(&[
            EditCommand::MoveToStart { select: false },
            EditCommand::MoveToLineEnd { select: false },
        ]);
        assert_eq!(rl.editor.insertion_point(), 2); // 'c', not 3 (the newline gap)
    }

    #[test]
    fn vi_normal_k_uses_prefix_search() {
        // `j`/`k` in vi normal mode should use prefix search instead of plain
        // history traversal
        let mut rl = seam_engine(Box::<crate::Vi>::default());

        let success_cond = "ls /tmp";

        rl.history
            .save(HistoryItem::from_command_line("ls -la"))
            .unwrap();
        rl.history
            .save(HistoryItem::from_command_line(success_cond))
            .unwrap();
        rl.history
            .save(HistoryItem::from_command_line("echo hi"))
            .unwrap();

        type_each(&mut rl, &[ch('l'), ch('s'), key(KeyCode::Esc)]);
        drive(&mut rl, &[ch('k')]);

        assert_eq!(rl.editor.get_buffer(), success_cond);
    }

    #[test]
    fn vi_normal_k_off_end_uses_plain_walk() {
        // The complement of the prefix case: once the caret leaves the buffer
        // end (here `h` steps it onto 'l'), history nav falls back to plain
        // bash-style traversal and returns the most recent entry overall, not a
        // prefix match.
        let mut rl = seam_engine(Box::<crate::Vi>::default());

        rl.history
            .save(HistoryItem::from_command_line("ls -la"))
            .unwrap();
        rl.history
            .save(HistoryItem::from_command_line("ls /tmp"))
            .unwrap();
        rl.history
            .save(HistoryItem::from_command_line("echo hi"))
            .unwrap();

        type_each(&mut rl, &[ch('l'), ch('s'), key(KeyCode::Esc)]);
        drive(&mut rl, &[ch('h')]); // caret off the end, onto 'l'
        drive(&mut rl, &[ch('k')]);

        assert_eq!(rl.editor.get_buffer(), "echo hi");
    }

    #[test]
    fn vi_hl_cross_newline_at_engine_seam() {
        // `h`/`l` keys, driven through the vi parser, cross the line terminator
        // under the default cross-line policy. Buffer "ab\ncd".
        let mut rl = seam_engine(Box::<crate::Vi>::default());
        rl.run_edit_commands(&[EditCommand::InsertString("ab\ncd".into())]);
        drive(&mut rl, &[key(KeyCode::Esc)]); // vi normal
        rl.run_edit_commands(&[EditCommand::MoveToLineStart { select: false }]);
        assert_eq!(rl.editor.insertion_point(), 3); // 'c', start of line 2
        drive(&mut rl, &[ch('h')]); // crosses up to 'b' (end of line 1)
        assert_eq!(rl.editor.insertion_point(), 1);
        drive(&mut rl, &[ch('l')]); // crosses down to 'c' (start of line 2)
        assert_eq!(rl.editor.insertion_point(), 3);
    }
}
