
A readline replacement written in Rust

## Example (Simple REPL)

```rust
// Create a default reedline to handle user input

use reedline::{Reedline, DefaultPrompt, Signal};

let mut line_editor = Reedline::new();
let prompt = Box::new(DefaultPrompt::default());

loop {
    let sig = line_editor.read_line(prompt.clone()).unwrap();
    match sig {
        Signal::CtrlD | Signal::CtrlC => {
            line_editor.print_crlf().unwrap();
            break;
        }
        Signal::Success(buffer) => {
            // process `buffer`
            println!("We processed: {}", buffer);
        }
        Signal::CtrlL => {
            line_editor.clear_screen().unwrap();
        }
    }
}

```
## Keybindings

```rust
// Configure reedline with custom keybindings

let mut keybindings = default_keybindings();
keybindings.add_binding(
    KeyModifiers::ALT,
    KeyCode::Char('m'),
    vec![EditCommand::BackspaceWord],
);

let mut line_editor = Reedline::new()
    .with_keybindings(keybindings);

```

## History

```rust
// Create a reedline with history support, including history size limits

let mut line_editor = Reedline::new()
    .with_history("history.txt", 5)?

```

## Are we prompt yet? (Development status)

This crate is currently under active development in JT's [live-coding streams](https://www.twitch.tv/jntrnr).
If you want to see a feature, jump by the streams, file an [issue](https://github.com/jonathandturner/reedline/issues) or contribute a [PR](https://github.com/jonathandturner/reedline/pulls)!

- [x] Basic unicode grapheme aware cursor editing.
- [x] Configurable prompt
- [x] Basic EMACS-style editing shortcuts.
- [ ] Advanced multiline unicode aware editing.
- [x] Configurable keybindings.
- [x] Basic system integration with clipboard or optional stored history file.
- [ ] Content aware highlighting or validation.
- [ ] Autocompletion.

For a more detailed roadmap check out [TODO.txt](https://github.com/jonathandturner/reedline/blob/main/TODO.txt).

### Alternatives

For currently more mature Rust line editing check out:

- [rustyline](https://crates.io/crates/rustyline)
