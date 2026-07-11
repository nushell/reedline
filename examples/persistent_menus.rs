// Create a reedline object with persistent menus
// cargo run --example persistent_menus
//
// "he" [Tab] opens the completion menu filtered to the "hello ..." entries.
// Erase with [Backspace] — even past "h" to an empty line — and the menu
// stays open, refiltering back up to the full list of options.
//
// Change `with_persistent_menus(true)` to `false` to see the default
// behavior: with quick completions on, the first [Backspace] dismisses the
// menu; without quick completions, the menu closes once the line is empty.

use reedline::{
    default_emacs_keybindings, ColumnarMenu, Completer, DefaultPrompt, Emacs, KeyCode,
    KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, Span,
    Suggestion,
};
use std::io;

// Prefix completer that also suggests on an empty line (like a shell does);
// `DefaultCompleter` returns nothing for empty input, which would make the
// kept-open menu invisible right when this example wants to show it.
struct PrefixCompleter {
    commands: Vec<String>,
}

impl Completer for PrefixCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let filter = &line[..pos];
        self.commands
            .iter()
            .filter(|command| command.starts_with(filter))
            .map(|command| Suggestion {
                value: command.clone(),
                span: Span::new(0, pos),
                ..Suggestion::default()
            })
            .collect()
    }
}

fn add_menu_keybindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
}

fn main() -> io::Result<()> {
    let commands = vec![
        "hello world".into(),
        "hello world reedline".into(),
        "hello world something".into(),
        "history 1".into(),
        "history 2".into(),
        "test".into(),
        "this is the reedline crate".into(),
        "clear".into(),
        "exit".into(),
        "logout".into(),
        "login".into(),
    ];

    let completer = Box::new(PrefixCompleter { commands });

    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let mut keybindings = default_emacs_keybindings();
    add_menu_keybindings(&mut keybindings);

    let edit_mode = Box::new(Emacs::new(keybindings));

    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_quick_completions(true)
        .with_persistent_menus(true)
        .with_edit_mode(edit_mode);

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
            _ => {}
        }
    }
}
