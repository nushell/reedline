// Demo of bottom margin feature for reserving space at the bottom of the terminal
// cargo run --example bottom_margin
//
// This example shows how to reserve space at the bottom of the terminal
// to ensure completion menus and hints have room to display.
//
// Try resizing the terminal and triggering completions with Tab to see
// how the margin affects menu positioning.

use reedline::{
    default_emacs_keybindings, BottomMargin, ColumnarMenu, DefaultCompleter, DefaultPrompt, Emacs,
    KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
};
use std::io;

fn main() -> io::Result<()> {
    println!("Bottom margin demo:");
    println!("This reserves space at the bottom of the terminal for completions/hints");
    println!("Abort with Ctrl-C or Ctrl-D");
    println!();

    let commands = vec![
        "help".into(),
        "hello".into(),
        "history".into(),
        "exit".into(),
        "clear".into(),
        "completions".into(),
        "bottom".into(),
        "margin".into(),
    ];

    let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

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

    // Configure reedline with a bottom margin
    // Try changing this between Fixed and Proportional to see the difference
    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode)
        .with_bottom_margin(BottomMargin::Fixed(5)); // Reserve 5 lines at bottom
                                                     // Alternative: .with_bottom_margin(BottomMargin::Proportional(0.3)); // 30% of screen

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                if buffer == "exit" {
                    println!("\nExiting!");
                    break Ok(());
                }
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
        }
    }
}
