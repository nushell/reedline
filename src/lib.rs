//! # reedline `\|/`
//! # A readline replacement written in Rust
//!
//! Reedline is a project to create a line editor (like bash's `readline` or
//! zsh's `zle`) that supports many of the modern conveniences of CLIs,
//! including syntax highlighting, completions, multiline support, Unicode
//! support, and more.  It is currently primarily developed as the interactive
//! editor for [nushell](https://github.com/nushell/nushell) (starting with
//! `v0.60`) striving to provide a pleasant interactive experience.
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
//!         Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
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
//!     default_vi_insert_keybindings, default_vi_normal_keybindings, EditMode, Reedline, Vi,
//! };
//!
//! let mut line_editor = Reedline::create().with_edit_mode(Box::new(Vi::new(
//!     default_vi_insert_keybindings(),
//!     default_vi_normal_keybindings(),
//! )));
//! ```
//!
//! ## Crate features
//!
//! - `clipboard`: Enable support to use the `SystemClipboard`. Enabling this feature will return a `SystemClipboard` instead of a local clipboard when calling `get_default_clipboard()`.
//! - `bashisms`: Enable support for special text sequences that recall components from the history. e.g. `!!` and `!$`. For use in shells like `bash` or [`nushell`](https://nushell.sh).
//! - `sqlite`: Provides the `SqliteBackedHistory` to store richer information in the history. Statically links the required sqlite version.
//! - `sqlite-dynlib`: Alternative to the feature `sqlite`. Will not statically link. Requires `sqlite >= 3.38` to link dynamically!
//! - `external_printer`: **Experimental:** Thread-safe `ExternalPrinter` handle to print lines from concurrently running threads.
//!
//! ## Are we prompt yet? (Development status)
//!
//! Reedline has now all the basic features to become the primary line editor for [nushell](https://github.com/nushell/nushell
//! )
//!
//! - General editing functionality, that should feel familiar coming from other shells (e.g. bash, fish, zsh).
//! - Configurable keybindings (emacs-style bindings and basic vi-style).
//! - Configurable prompt
//! - Content-aware syntax highlighting.
//! - Autocompletion (With graphical selection menu or simple cycling inline).
//! - History with interactive search options (optionally persists to file, can support multilple sessions accessing the same file)
//! - Fish-style history autosuggestion hints
//! - Undo support.
//! - Clipboard integration
//! - Line completeness validation for seamless entry of multiline command sequences.
//!
//! ### Areas for future improvements
//!
//! - [ ] Support for Unicode beyond simple left-to-right scripts
//! - [ ] Easier keybinding configuration
//! - [ ] Support for more advanced vi commands
//! - [ ] Visual selection
//! - [ ] Smooth experience if completion or prompt content takes long to compute
//! - [ ] Support for a concurrent output stream from background tasks to be displayed, while the input prompt is active. ("Full duplex" mode)
//!
//! For more ideas check out the [feature discussion](https://github.com/nushell/reedline/issues/63) or hop on the `#reedline` channel of the [nushell discord](https://discordapp.com/invite/NtAbbGn).
//!
//! ### Development history
//!
//! If you want to follow along with the history how reedline got started, you can watch the [recordings](https://youtube.com/playlist?list=PLP2yfE2-FXdQw0I6O4YdIX_mzBeF5TDdv) of [JT](https://github.com/jntrnr)'s [live-coding streams](https://www.twitch.tv/jntrnr).
//!
//! [Playlist: Creating a line editor in Rust](https://youtube.com/playlist?list=PLP2yfE2-FXdQw0I6O4YdIX_mzBeF5TDdv)
//!
//! ### Alternatives
//!
//! For currently more mature Rust line editing check out:
//!
//! - [rustyline](https://crates.io/crates/rustyline)
//!
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(missing_docs)]
// #![deny(warnings)]
mod core_editor;
pub use core_editor::Editor;
pub use core_editor::LineBuffer;

mod enums;
pub use enums::{EditCommand, ReedlineEvent, ReedlineRawEvent, Signal, UndoBehavior};

mod painting;
pub use painting::{Painter, StyledText};

mod engine;
pub use engine::Reedline;

mod result;
pub use result::{ReedlineError, ReedlineErrorVariants, Result};

mod history;
#[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
pub use history::SqliteBackedHistory;
pub use history::{
    CommandLineSearch, FileBackedHistory, History, HistoryItem, HistoryItemId,
    HistoryNavigationQuery, HistorySessionId, SearchDirection, SearchFilter, SearchQuery,
    HISTORY_SIZE,
};

mod prompt;
pub use prompt::{
    DefaultPrompt, DefaultPromptSegment, Prompt, PromptEditMode, PromptHistorySearch,
    PromptHistorySearchStatus, PromptViMode,
};

mod edit_mode;
pub use edit_mode::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    CursorConfig, EditMode, Emacs, Keybindings, Vi,
};

mod highlighter;
pub use highlighter::{ExampleHighlighter, Highlighter, SimpleMatchHighlighter};

mod completion;
pub use completion::{Completer, DefaultCompleter, Span, Suggestion};

mod hinter;
pub use hinter::CwdAwareHinter;
pub use hinter::{DefaultHinter, Hinter};

mod validator;
pub use validator::{DefaultValidator, ValidationResult, Validator};

mod menu;
pub use menu::{
    menu_functions, ColumnarMenu, DescriptionMenu, DescriptionMode, IdeMenu, ListMenu, Menu,
    MenuBuilder, MenuEvent, MenuTextStyle, ReedlineMenu,
};

mod terminal_extensions;
pub use terminal_extensions::kitty_protocol_available;

mod utils;

mod external_printer;
pub use utils::{
    get_reedline_default_keybindings, get_reedline_edit_commands,
    get_reedline_keybinding_modifiers, get_reedline_keycodes, get_reedline_prompt_edit_modes,
    get_reedline_reedline_events,
};

// Reexport the key types to be independent from an explicit crossterm dependency.
pub use crossterm::{
    event::{KeyCode, KeyModifiers},
    style::Color,
};
#[cfg(feature = "external_printer")]
pub use external_printer::ExternalPrinter;
