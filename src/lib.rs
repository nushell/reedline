//! # reedline `\|/`
//! # A readline replacement written in Rust
//!
//! Reedline is a project to create a readline-style crate
//! for Rust that supports many of the modern conveniences of CLIs,
//! including syntax highlighting, completions, multiline support,
//! Unicode support, and more.
//!
//! ## Basic example
//!
//! ```rust,no_run
//! // Create a default reedline object to handle user input
//!
//! use reedline::{DefaultPrompt, Reedline, Signal};
//! use std::io;
//!
//!  let mut line_editor = Reedline::create()?;
//!  let prompt = DefaultPrompt::default();
//!
//!  loop {
//!      let sig = line_editor.read_line(&prompt);
//!      match sig {
//!          Ok(Signal::Success(buffer)) => {
//!              println!("We processed: {}", buffer);
//!          }
//!          Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
//!              println!("\nAborted!");
//!              break;
//!          }
//!          Ok(Signal::CtrlL) => {
//!              line_editor.clear_screen();
//!          }
//!          x => {
//!              println!("Event: {:?}", x);
//!          }
//!      }
//!  }
//! # Ok::<(), io::Error>(())
//! ```
//! ## Integrate with custom Keybindings
//!
//! ```rust,no_run
//! // Configure reedline with custom keybindings
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    crossterm = "*"
//!
//! use std::io;
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
//! let mut line_editor = Reedline::create()?.with_edit_mode(edit_mode);
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Integrate with custom History
//!
//! ```rust,no_run
//! // Create a reedline object with history support, including history size limits
//!
//! use std::io;
//! use reedline::{FileBackedHistory, Reedline};
//!
//! let history = Box::new(
//!     FileBackedHistory::with_file(5, "history.txt".into())
//!         .expect("Error configuring history with file"),
//! );
//! let mut line_editor = Reedline::create()?
//!     .with_history(history)
//!     .expect("Error configuring reedline with history");
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Integrate with custom Highlighter
//!
//! ```rust,no_run
//! // Create a reedline object with highlighter support
//!
//! use std::io;
//! use reedline::{ExampleHighlighter, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let mut line_editor =
//! Reedline::create()?.with_highlighter(Box::new(ExampleHighlighter::new(commands)));
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Integrate with custom tab completion
//!
//! ```rust,no_run
//! // Create a reedline object with tab completions support
//!
//! use std::io;
//! use reedline::{DefaultCompleter, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
//!
//! let mut line_editor = Reedline::create()?.with_completer(completer);
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Integrate with custom Hinter
//!
//! ```rust,no_run
//! // Create a reedline object with in-line hint support
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    nu-ansi-term = "*"
//!
//! use std::io;
//! use {
//!   nu_ansi_term::{Color, Style},
//!   reedline::{DefaultHinter, Reedline},
//! };
//!
//!
//! let mut line_editor = Reedline::create()?.with_hinter(Box::new(
//!   DefaultHinter::default()
//!   .with_style(Style::new().italic().fg(Color::LightGray)),
//! ));
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Are we prompt yet? (Development status)
//!
//! This crate is currently under active development
//! in JT's [live-coding streams](https://www.twitch.tv/jntrnr).
//! If you want to see a feature, jump by the streams,
//! file an [issue](https://github.com/nushell/reedline/issues)
//! or contribute a [PR](https://github.com/nushell/reedline/pulls)!
//!
//! - [x] Basic unicode grapheme aware cursor editing.
//! - [x] Configurable prompt
//! - [x] Basic EMACS-style editing shortcuts.
//! - [x] Configurable keybindings.
//! - [x] Basic system integration with clipboard or optional stored history file.
//! - [x] Content aware highlighting.
//! - [x] Autocompletion.
//! - [x] Undo support.
//! - [x] Multiline aware editing with line completion validation.
//!
//! For a more detailed roadmap check out [TODO.txt](https://github.com/nushell/reedline/blob/main/TODO.txt).
//!
//! Join the vision discussion in the [vision milestone list](https://github.com/nushell/reedline/milestone/1) by contributing suggestions or voting.
//!
//! ### Alternatives
//!
//! For currently more mature Rust line editing check out:
//!
//! - [rustyline](https://crates.io/crates/rustyline)
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(rustdoc::missing_doc_code_examples)]
#![warn(missing_docs)]
// #![deny(warnings)]
mod core_editor;
pub use core_editor::LineBuffer;

mod text_manipulation;

mod enums;
pub use enums::{EditCommand, ReedlineEvent, Signal, UndoBehavior};

mod painter;

mod engine;
pub use engine::Reedline;

mod history;
pub use history::{FileBackedHistory, History, HistoryNavigationQuery, HISTORY_SIZE};

mod prompt;
pub use prompt::{
    DefaultPrompt, Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus,
    PromptViMode, DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR,
};

mod edit_mode;
pub use edit_mode::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    EditMode, Emacs, Keybindings, Vi,
};

mod highlighter;
pub use highlighter::{ExampleHighlighter, Highlighter, SimpleMatchHighlighter};

mod styled_text;
pub use styled_text::StyledText;

mod completion;
pub use completion::{Completer, DefaultCompleter, Span};

mod hinter;
pub use hinter::{DefaultHinter, Hinter};

mod validator;
pub use validator::{DefaultValidator, ValidationResult, Validator};

mod menu;
pub use menu::{CompletionMenu, HistoryMenu, Menu};

mod internal;
pub use internal::{
    get_reedline_default_keybindings, get_reedline_edit_commands,
    get_reedline_keybinding_modifiers, get_reedline_keycodes, get_reedline_prompt_edit_modes,
    get_reedline_reedline_events,
};
