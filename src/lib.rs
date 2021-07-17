//! # reedline `\|/`
//! # A readline replacement written in Rust
//!
//! ## Basic example
//!
//! ```rust,no_run
//! // Create a default reedline object to handle user input
//!
//! use reedline::{DefaultPrompt, Reedline, Signal};
//!
//! fn main() {
//!     let mut line_editor = Reedline::new();
//!     let prompt = DefaultPrompt::default();
//!
//!     loop {
//!         let sig = line_editor.read_line(&prompt).unwrap();
//!         match sig {
//!             Signal::Success(buffer) => {
//!                 println!("We processed: {}", buffer);
//!             }
//!             Signal::CtrlD | Signal::CtrlC => {
//!                 line_editor.print_crlf().unwrap();
//!                 break;
//!             }
//!             Signal::CtrlL => {
//!                 line_editor.clear_screen().unwrap();
//!             }
//!         }
//!     }
//! }
//! ```
//! ## Integrate with custom Keybindings
//!
//! ```rust
//! // Configure reedline with custom keybindings
//!
//! //Cargo.toml
//! //	[dependencies]
//! //	crossterm = "*"
//!
//! use {
//!   crossterm::event::{KeyCode, KeyModifiers},
//!   reedline::{default_emacs_keybindings, EditCommand, Reedline},
//! };
//!
//! let mut keybindings = default_emacs_keybindings();
//! keybindings.add_binding(
//! 	KeyModifiers::ALT,
//!   KeyCode::Char('m'),
//!   vec![EditCommand::BackspaceWord],
//! );
//!
//! let mut line_editor = Reedline::new().with_keybindings(keybindings);
//! ```
//!
//! ## Integrate with custom History
//!
//! ```rust,no_run
//! // Create a reedline object with history support, including history size limits
//!
//! use reedline::{FileBackedHistory, Reedline};
//!
//! let history = Box::new(
//!   FileBackedHistory::with_file(5, "history.txt".into())
//!   	.expect("Error configuring history with file"),
//! );
//! let mut line_editor = Reedline::new()
//! 	.with_history(history)
//! 	.expect("Error configuring reedline with history");
//! ```
//!
//! ## Integrate with custom Highlighter
//!
//! ```rust
//! // Create a reedline object with highlighter support
//!
//! use reedline::{DefaultHighlighter, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let mut line_editor =
//! Reedline::new().with_highlighter(Box::new(DefaultHighlighter::new(commands)));
//! ```
//!
//! ## Integrate with custom Tab-Handler
//!
//! ```rust
//! // Create a reedline object with tab completions support
//!
//! use reedline::{DefaultCompleter, DefaultTabHandler, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
//!
//! let mut line_editor = Reedline::new().with_tab_handler(Box::new(
//!   DefaultTabHandler::default().with_completer(completer),
//! ));
//! ```
//!
//! ## Integrate with custom Hinter
//!
//! ```rust
//! // Create a reedline object with in-line hint support
//!
//! //Cargo.toml
//! //	[dependencies]
//! //	nu-ansi-term = "*"
//!
//! use {
//!   nu_ansi_term::{Color, Style},
//!   reedline::{DefaultCompleter, DefaultHinter, Reedline},
//! };
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
//!
//! let mut line_editor = Reedline::new().with_hinter(Box::new(
//!   DefaultHinter::default()
//!   .with_completer(completer) // or .with_history()
//!   // .with_inside_line()
//!   .with_style(Style::new().italic().fg(Color::LightGray)),
//! ));
//! ```
//!
//! ## Integrate with custom Edit Mode
//!
//! ```rust
//! // Create a reedline object with custom edit mode
//!
//! use reedline::{EditMode, Reedline};
//!
//! let mut line_editor = Reedline::new().with_edit_mode(
//!   EditMode::ViNormal, // or EditMode::Emacs or EditMode::ViInsert
//! );
//! ```
//!
//! ## Are we prompt yet? (Development status)
//!
//! This crate is currently under active development
//! in JT's [live-coding streams](https://www.twitch.tv/jntrnr).
//! If you want to see a feature, jump by the streams,
//! file an [issue](https://github.com/jntrnr/reedline/issues)
//! or contribute a [PR](https://github.com/jntrnr/reedline/pulls)!
//!
//! - [x] Basic unicode grapheme aware cursor editing.
//! - [x] Configurable prompt
//! - [x] Basic EMACS-style editing shortcuts.
//! - [x] Configurable keybindings.
//! - [x] Basic system integration with clipboard or optional stored history file.
//! - [x] Content aware highlighting or validation.
//! - [x] Autocompletion.
//! - [ ] Advanced multiline unicode aware editing.
//!
//! For a more detailed roadmap check out [TODO.txt](https://github.com/jntrnr/reedline/blob/main/TODO.txt).
//!
//! Join the vision discussion in the [vision milestone list](https://github.com/jntrnr/reedline/milestone/1) by contributing suggestions or voting.
//!
//! ### Alternatives
//!
//! For currently more mature Rust line editing check out:
//!
//! - [rustyline](https://crates.io/crates/rustyline)
mod clip_buffer;

mod text_manipulation;

mod enums;
pub use enums::{EditCommand, EditMode, Signal};

mod painter;

mod engine;
pub use engine::Reedline;

mod history;
pub use history::{FileBackedHistory, History, HISTORY_SIZE};

mod prompt;
pub use prompt::{
    DefaultPrompt, Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus,
    PromptViMode, DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR,
};

mod line_buffer;

mod keybindings;
pub use keybindings::default_emacs_keybindings;

mod syntax_highlighting_fileio;

mod vi_engine;
pub use vi_engine::ViEngine;

mod highlighter;
pub use highlighter::{DefaultHighlighter, Highlighter};

mod styled_text;

mod completer;
pub use completer::{Completer, DefaultCompleter, DefaultTabHandler, Span, TabHandler};

mod hinter;
pub use hinter::{DefaultHinter, Hinter};
