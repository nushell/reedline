//! # reedline `\|/`
//! # A readline replacement written in Rust
//!
//! Reedline is a line editor in the tradition of bash's `readline` and zsh's
//! `zle`, built for what people expect from a modern shell: syntax
//! highlighting, completions, interactive menus, history hints, multiline
//! editing and more.  It is currently primarily developed as the interactive
//! editor for [nushell](https://github.com/nushell/nushell), striving to
//! provide a pleasant interactive experience.
//!
//! ## Basic example
//!
//! ```rust,no_run
//! // Create a default reedline object to handle user input
//!
//! use reedline::{DefaultPrompt, Reedline, Signal};
//!
//! let mut line_editor = Reedline::create();
//! let prompt = DefaultPrompt::default();
//!
//! loop {
//!     let sig = line_editor.read_line(&prompt);
//!     match sig {
//!         Ok(Signal::Success(buffer)) => {
//!             println!("We processed: {}", buffer);
//!         }
//!         Ok(Signal::CtrlD) | Ok(Signal::CtrlC(_)) => {
//!             println!("\nAborted!");
//!             break;
//!         }
//!         x => {
//!             println!("Event: {:?}", x);
//!         }
//!     }
//! }
//! ```
//! ## Integrate with custom keybindings
//!
//! ```rust
//! // Configure reedline with custom keybindings
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    crossterm = "*"
//!
//! use {
//!   crossterm::event::{KeyCode, KeyModifiers},
//!   reedline::{default_emacs_keybindings, EditCommand, Reedline, Emacs, ReedlineEvent},
//! };
//!
//! let mut keybindings = default_emacs_keybindings();
//! keybindings.add_binding(
//!     KeyModifiers::ALT,
//!     KeyCode::Char('m'),
//!     ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
//! );
//! let edit_mode = Box::new(Emacs::new(keybindings));
//!
//! let mut line_editor = Reedline::create().with_edit_mode(edit_mode);
//! ```
//!
//! ## Integrate with [`History`]
//!
//! ```rust,no_run
//! // Create a reedline object with history support, including history size limits
//!
//! use reedline::{FileBackedHistory, Reedline};
//!
//! let history = Box::new(
//!     FileBackedHistory::with_file(5, "history.txt".into())
//!         .expect("Error configuring history with file"),
//! );
//! let mut line_editor = Reedline::create()
//!     .with_history(history);
//! ```
//!
//! ## Integrate with custom syntax [`Highlighter`]
//!
//! ```rust
//! // Create a reedline object with highlighter support
//!
//! use reedline::{ExampleHighlighter, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let mut line_editor =
//! Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
//! ```
//!
//! ## Integrate with custom tab completion
//!
//! ```rust
//! // Create a reedline object with tab completions support
//!
//! use reedline::{default_emacs_keybindings, ColumnarMenu, DefaultCompleter, Emacs, KeyCode, KeyModifiers, Reedline, ReedlineEvent, ReedlineMenu, MenuBuilder};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
//! // Use the interactive menu to select options from the completer
//! let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
//! // Set up the required keybindings
//! let mut keybindings = default_emacs_keybindings();
//! keybindings.add_binding(
//!     KeyModifiers::NONE,
//!     KeyCode::Tab,
//!     ReedlineEvent::UntilFound(vec![
//!         ReedlineEvent::Menu("completion_menu".to_string()),
//!         ReedlineEvent::MenuNext,
//!     ]),
//! );
//!
//! let edit_mode = Box::new(Emacs::new(keybindings));
//!
//! let mut line_editor = Reedline::create()
//!     .with_completer(completer)
//!     .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
//!     .with_edit_mode(edit_mode);
//! ```
//!
//! ## Integrate with [`Hinter`] for fish-style history autosuggestions
//!
//! ```rust
//! // Create a reedline object with in-line hint support
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    nu-ansi-term = "*"
//!
//! use {
//!   nu_ansi_term::{Color, Style},
//!   reedline::{DefaultHinter, Reedline},
//! };
//!
//!
//! let mut line_editor = Reedline::create().with_hinter(Box::new(
//!   DefaultHinter::default()
//!   .with_style(Style::new().italic().fg(Color::LightGray)),
//! ));
//! ```
//!
//!
//! ## Integrate with custom line completion [`Validator`]
//!
//! ```rust
//! // Create a reedline object with line completion validation support
//!
//! use reedline::{DefaultValidator, Reedline};
//!
//! let validator = Box::new(DefaultValidator);
//!
//! let mut line_editor = Reedline::create().with_validator(validator);
//! ```
//!
//! ## Use custom [`EditMode`]
//!
//! ```rust
//! // Create a reedline object with custom edit mode
//! // This can define a keybinding setting or enable vi-emulation
//! use reedline::{
//!     default_vi_insert_keybindings, default_vi_normal_keybindings,
//!     default_vi_visual_keybindings, EditMode, Reedline, Vi,
//! };
//!
//! let mut line_editor = Reedline::create().with_edit_mode(Box::new(Vi::new(
//!     default_vi_insert_keybindings(),
//!     default_vi_normal_keybindings(),
//!     default_vi_visual_keybindings(),
//! )));
//! ```
//!
//! ## Switch between edit modes at runtime
//!
//! Register every machine a keybinding may name; the one passed to
//! `with_edit_mode` starts active, and a `SwitchMode` event activates another.
//!
//! ```rust
//! use reedline::{
//!     default_emacs_keybindings, Emacs, Helix, KeyCode, KeyModifiers, PromptEditMode,
//!     PromptHelixMode, Reedline, ReedlineEvent,
//! };
//!
//! let mut emacs = default_emacs_keybindings();
//! emacs.add_binding(
//!     KeyModifiers::CONTROL,
//!     KeyCode::Char('h'),
//!     ReedlineEvent::SwitchMode(PromptEditMode::Helix(PromptHelixMode::Normal)),
//! );
//!
//! let mut line_editor = Reedline::create()
//!     .with_edit_mode(Box::new(Emacs::new(emacs)))
//!     .with_additional_edit_mode(Box::new(Helix::default()));
//! ```
//!
//! ## Enable mouse click-to-cursor
//!
//! ```rust,no_run
//! use reedline::{MouseClickMode, Reedline};
//!
//! let mut line_editor =
//!     Reedline::create().with_mouse_click(MouseClickMode::EnabledWithOsc133);
//! ```
//!
//! ## Use `Helix` edit mode
//!
//! Selection-first editing: motions carry the selection, verbs act on it.
//!
//! ```rust
//! use reedline::{default_helix_normal_keybindings, Helix, Reedline};
//!
//! let mut normal_keybindings = default_helix_normal_keybindings();
//! // normal_keybindings.add_binding(..);
//!
//! let line_editor = Reedline::create().with_edit_mode(Box::new(
//!     Helix::default().with_normal_keybindings(normal_keybindings),
//! ));
//! ```
//!
//! Run `cargo run --example helix` for the mode on its own, or
//! `cargo run --example demo -- --helix` to exercise it against the demo's
//! history and menus.
//!
//! ## Crate features
//!
//! All features are off by default.
//!
//! - `system_clipboard`: cut, copy and paste to the system clipboard, beside reedline's own cut buffer.
//! - `bashisms`: history expansion such as `!!` and `!$`.
//! - `sqlite`: adds `SqliteBackedHistory`, which stores richer information about each entry. Statically links SQLite.
//! - `sqlite-dynlib`: the same, linked dynamically. Requires `sqlite >= 3.45.3`.
//! - `libc`: makes crossterm use `libc` instead of `rustix` for its terminal calls.
//!
//! ## Community and roadmap
//!
//! Come talk to us in the `#reedline` channel of the [Nushell Discord](https://discord.gg/NtAbbGn).
//!
//! Where reedline is heading is tracked in the roadmap issues:
//!
//! - [Prompt, history, hints and the API](https://github.com/nushell/reedline/issues/1284)
//! - [Edit modes: emacs, vi and helix](https://github.com/nushell/reedline/issues/1221)
//! - [Completion menu: actions, insertion and display](https://github.com/nushell/reedline/issues/1283)
//!
//! ## Alternatives
//!
//! - [rustyline](https://crates.io/crates/rustyline)
//!
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(missing_docs)]
// #![deny(warnings)]
mod core_editor;
pub use core_editor::{Editor, LineBuffer};

mod auto_pairs;
pub use auto_pairs::{AutoPairAction, AutoPairContext, AutoPairs};

mod enums;
pub use enums::{
    Direction, EditCommand, EditCommandDiscriminants, EventStatus, FindStop, Granularity,
    MotionTarget, MouseButton, ReedlineEvent, ReedlineEventDiscriminants, ReedlineRawEvent, Signal,
    TextObject, TextObjectBracket, TextObjectQuote, TextObjectScope, TextObjectType, UndoBehavior,
    WordEdge, WordKind,
};

mod painting;
pub use painting::{Painter, StyledText};

mod engine;
pub use engine::{EditorCommand, MouseClickMode, Reedline, RepaintSignal};

#[cfg(feature = "system_clipboard")]
mod paste_interceptor;
#[cfg(feature = "system_clipboard")]
pub use paste_interceptor::{PasteAction, PasteInterceptor};

mod paste_burst_hook;
pub use paste_burst_hook::PasteBurstHook;

mod result;
pub use result::{ReedlineError, ReedlineErrorVariants, Result};

mod history;
#[cfg(feature = "_sqlite")]
pub use history::SqliteBackedHistory;
pub use history::{
    CommandLineSearch, FileBackedHistory, History, HistoryItem, HistoryItemExtraInfo,
    HistoryItemId, HistoryNavigationQuery, HistorySessionId, HistoryTimestamp, IgnoreAllExtraInfo,
    JsonFilterValue, SearchDirection, SearchFilter, SearchQuery, HISTORY_SIZE,
};

mod prompt;
pub use prompt::PromptHelixMode;
pub use prompt::{
    DefaultPrompt, DefaultPromptSegment, Prompt, PromptEditMode, PromptEditModeDiscriminants,
    PromptHistorySearch, PromptHistorySearchStatus, PromptViMode, DEFAULT_INDICATOR_COLOR,
    DEFAULT_INSERT_PROMPT_INDICATOR, DEFAULT_MULTILINE_INDICATOR, DEFAULT_NORMAL_PROMPT_INDICATOR,
    DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR, DEFAULT_PROMPT_MULTILINE_COLOR,
    DEFAULT_PROMPT_RIGHT_COLOR, DEFAULT_SELECT_PROMPT_INDICATOR,
};

mod edit_mode;
pub use edit_mode::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    default_vi_visual_keybindings, CursorConfig, EditMode, Emacs, Keybindings, Vi,
};
pub use edit_mode::{
    default_helix_insert_keybindings, default_helix_normal_keybindings,
    default_helix_select_keybindings, Helix,
};

mod highlighter;
pub use highlighter::{AbbrExpandContext, ExampleHighlighter, Highlighter, SimpleMatchHighlighter};

mod completion;
pub use completion::{
    Completer, CompletionOrigin, CompletionResult, CompletionStatus, DefaultCompleter, Partial,
    Span, Suggestion, Suggestions,
};

mod hinter;
pub use hinter::CwdAwareHinter;
pub use hinter::{DefaultHinter, Hinter};

mod validator;
pub use validator::{DefaultValidator, ValidationResult, Validator};

mod status_line;
pub use status_line::StatusLine;

mod menu;
pub use menu::{
    menu_functions, ColumnarMenu, DescriptionMenu, DescriptionMode, DescriptionPosition, IdeMenu,
    InputMode, ListMenu, Menu, MenuBuilder, MenuEvent, MenuSettings, MenuTextStyle, OutputMode,
    ReedlineMenu, TraversalDirection,
};

mod terminal_extensions;
pub use terminal_extensions::kitty_protocol_available;
pub use terminal_extensions::semantic_prompt::{
    Osc133ClickEventsMarkers, Osc133Markers, Osc633Markers, PromptKind, SemanticPromptMarkers,
};

mod utils;

mod external_printer;
pub use utils::{
    get_reedline_default_keybindings, get_reedline_keybinding_modifiers, get_reedline_keycodes,
};

// Reexport the key types to be independent from an explicit crossterm dependency.
pub use crossterm::event::{KeyCode, KeyModifiers};
pub use external_printer::{ExternalOutput, ExternalPrinter};
pub use nu_ansi_term::Color;
