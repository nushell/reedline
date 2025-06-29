# A feature-rich line editor - powering Nushell

![GitHub](https://img.shields.io/github/license/nushell/reedline)
[![Crates.io](https://img.shields.io/crates/v/reedline)](https://crates.io/crates/reedline)
[![docs.rs](https://img.shields.io/docsrs/reedline)](https://docs.rs/reedline/)
![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/nushell/reedline/ci.yml?branch=main)
[![codecov](https://codecov.io/gh/nushell/reedline/graph/badge.svg?token=NUTC465WOL)](https://codecov.io/gh/nushell/reedline)
[![Discord](https://img.shields.io/discord/601130461678272522.svg?logo=discord)](https://discord.gg/NtAbbGn)

## Introduction

Reedline is a project to create a line editor (like bash's `readline` or zsh's `zle`) that supports many of the modern conveniences of CLIs, including syntax highlighting, completions, multiline support, Unicode support, and more.
It is currently primarily developed as the interactive editor for [nushell](https://github.com/nushell/nushell) (starting with `v0.60`) striving to provide a pleasant interactive experience.

## Outline

- [Examples](#examples)
  - [Basic example](#basic-example)
  - [Integrate with custom keybindings](#integrate-with-custom-keybindings)
  - [Integrate with `History`](#integrate-with-history)
  - [Integrate with custom syntax `Highlighter`](#integrate-with-custom-syntax-highlighter)
  - [Integrate with custom tab completion](#integrate-with-custom-tab-completion)
  - [Integrate with `Hinter` for fish-style history autosuggestions](#integrate-with-hinter-for-fish-style-history-autosuggestions)
  - [Integrate with custom line completion `Validator`](#integrate-with-custom-line-completion-validator)
  - [Use custom `EditMode`](#use-custom-editmode)
- [Crate features](#crate-features)
- [Are we prompt yet? (Development status)](#are-we-prompt-yet-development-status)
- [Contributing](./CONTRIBUTING.md)
- [Alternatives](#alternatives)

## Examples

For the full documentation visit <https://docs.rs/reedline>. The examples should highlight how you enable the most important features or which traits can be implemented for language-specific behavior.

### Basic example

```rust,no_run
// Create a default reedline object to handle user input

use reedline::{DefaultPrompt, Reedline, Signal};

let mut line_editor = Reedline::create();
let prompt = DefaultPrompt::default();

loop {
    let sig = line_editor.read_line(&prompt);
    match sig {
        Ok(Signal::Success(buffer)) => {
            println!("We processed: {}", buffer);
        }
        Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
            println!("\nAborted!");
            break;
        }
        x => {
            println!("Event: {:?}", x);
        }
    }
}
```

### Integrate with custom keybindings

```rust
// Configure reedline with custom keybindings

//Cargo.toml
//    [dependencies]
//    crossterm = "*"

use {
  crossterm::event::{KeyCode, KeyModifiers},
  reedline::{default_emacs_keybindings, EditCommand, Reedline, Emacs, ReedlineEvent},
};

let mut keybindings = default_emacs_keybindings();
keybindings.add_binding(
    KeyModifiers::ALT,
    KeyCode::Char('m'),
    ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
);
let edit_mode = Box::new(Emacs::new(keybindings));

let mut line_editor = Reedline::create().with_edit_mode(edit_mode);
```

### Integrate with `History`

```rust,no_run
// Create a reedline object with history support, including history size limits

use reedline::{FileBackedHistory, Reedline};

let history = Box::new(
  FileBackedHistory::with_file(5, "history.txt".into())
    .expect("Error configuring history with file"),
);
let mut line_editor = Reedline::create()
  .with_history(history);
```

### Integrate with custom syntax `Highlighter`

```rust
// Create a reedline object with highlighter support

use reedline::{ExampleHighlighter, Reedline};

let commands = vec![
  "test".into(),
  "hello world".into(),
  "hello world reedline".into(),
  "this is the reedline crate".into(),
];
let mut line_editor =
Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
```

### Integrate with custom tab completion

```rust
// Create a reedline object with tab completions support

use reedline::{default_emacs_keybindings, ColumnarMenu, DefaultCompleter, Emacs, KeyCode, KeyModifiers, Reedline, ReedlineEvent, ReedlineMenu};

let commands = vec![
  "test".into(),
  "hello world".into(),
  "hello world reedline".into(),
  "this is the reedline crate".into(),
];
let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
// Use the interactive menu to select options from the completer
let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
// Set up the required keybindings
let mut keybindings = default_emacs_keybindings();
keybindings.add_binding(
    KeyModifiers::NONE,
    KeyCode::Tab,
    ReedlineEvent::UntilFound(vec![
        ReedlineEvent::Menu("completion_menu".to_string()),
        ReedlineEvent::MenuNext,
    ]),
);

let edit_mode = Box::new(Emacs::new(keybindings));

let mut line_editor = Reedline::create()
    .with_completer(completer)
    .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
    .with_edit_mode(edit_mode);
```

### Integrate with `Hinter` for fish-style history autosuggestions

```rust
// Create a reedline object with in-line hint support

//Cargo.toml
//  [dependencies]
//  nu-ansi-term = "*"

use {
  nu_ansi_term::{Color, Style},
  reedline::{DefaultHinter, Reedline},
};

let mut line_editor = Reedline::create().with_hinter(Box::new(
  DefaultHinter::default()
  .with_style(Style::new().italic().fg(Color::LightGray)),
));
```

### Integrate with custom line completion `Validator`

```rust
// Create a reedline object with line completion validation support

use reedline::{DefaultValidator, Reedline};

let validator = Box::new(DefaultValidator);

let mut line_editor = Reedline::create().with_validator(validator);
```

### Use custom `EditMode`

```rust
// Create a reedline object with custom edit mode
// This can define a keybinding setting or enable vi-emulation

use reedline::{
    default_vi_insert_keybindings, default_vi_normal_keybindings, EditMode, Reedline, Vi,
};

let mut line_editor = Reedline::create().with_edit_mode(Box::new(Vi::new(
    default_vi_insert_keybindings(),
    default_vi_normal_keybindings(),
)));
```

## Crate features

- `clipboard`: Enable support to use the `SystemClipboard`. Enabling this feature will return a `SystemClipboard` instead of a local clipboard when calling `get_default_clipboard()`.
- `bashisms`: Enable support for special text sequences that recall components from the history. e.g. `!!` and `!$`. For use in shells like `bash` or [`nushell`](https://nushell.sh).
- `sqlite`: Provides the `SqliteBackedHistory` to store richer information in the history. Statically links the required sqlite version.
- `sqlite-dynlib`: Alternative to the feature `sqlite`. Will not statically link. Requires `sqlite >= 3.38` to link dynamically!
- `external_printer`: **Experimental:** Thread-safe `ExternalPrinter` handle to print lines from concurrently running threads.

## Are we prompt yet? (Development status)

Reedline has now all the basic features to become the primary line editor for [nushell](https://github.com/nushell/nushell
)

- General editing functionality, that should feel familiar coming from other shells (e.g. bash, fish, zsh).
- Configurable keybindings (emacs-style bindings and basic vi-style).
- Configurable prompt
- Content-aware syntax highlighting.
- Autocompletion (With graphical selection menu or simple cycling inline).
- History with interactive search options (optionally persists to file, can support multiple sessions accessing the same file)
- Fish-style history autosuggestion hints
- Undo support.
- Clipboard integration
- Line completeness validation for seamless entry of multiline command sequences.
- Visual selection

### Areas for future improvements

- [ ] Support for Unicode beyond simple left-to-right scripts
- [ ] Easier keybinding configuration
- [ ] Support for more advanced vi commands
- [ ] Smooth experience if completion or prompt content takes long to compute
- [ ] Support for a concurrent output stream from background tasks to be displayed, while the input prompt is active. ("Full duplex" mode)

For more ideas check out the [feature discussion](https://github.com/nushell/reedline/issues/63) or hop on the `#reedline` channel of the [nushell discord](https://discordapp.com/invite/NtAbbGn).

### Alternatives

For currently more mature Rust line editing check out:

- [rustyline](https://crates.io/crates/rustyline)
