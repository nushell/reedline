// Example demonstrating multi-keystroke chord support in reedline
//
// This example shows how to configure key chords (multi-key sequences)
// that trigger actions. For example, pressing Ctrl+X followed by Ctrl+C
// can be bound to a specific event.

use crossterm::event::{KeyCode, KeyModifiers};
use reedline::{
    default_emacs_keybindings, DefaultPrompt, Emacs, KeyCombination, Reedline, ReedlineEvent,
    Signal,
};
use std::io;

/// Helper to create a KeyCombination for Ctrl+<char>
fn ctrl(c: char) -> KeyCombination {
    KeyCombination {
        modifier: KeyModifiers::CONTROL,
        key_code: KeyCode::Char(c),
    }
}

fn main() -> io::Result<()> {
    println!("Reedline Chord Example");
    println!("======================");
    println!();
    println!("This example demonstrates multi-keystroke chord bindings.");
    println!();
    println!("Available chords:");
    println!("  Ctrl+X Ctrl+C - Quit");
    println!("  Ctrl+X Ctrl+Y Ctrl+Z Ctrl+Z Ctrl+Y - Report that nothing happens");
    println!();
    println!("Regular keys and single-key bindings still work normally.");
    println!("You may also type 'exit' or press Ctrl+D to quit.");
    println!();

    // Start with the default Emacs keybindings
    let mut keybindings = default_emacs_keybindings();

    // Add chord bindings
    // Ctrl+X Ctrl+C: Quit
    keybindings.add_sequence_binding(&[ctrl('x'), ctrl('c')], ReedlineEvent::CtrlD);

    // Ctrl+X Ctrl+Y Ctrl+Z Ctrl+Z Ctrl+Y: Quit
    keybindings.add_sequence_binding(
        &[ctrl('x'), ctrl('y'), ctrl('z'), ctrl('z'), ctrl('y')],
        ReedlineEvent::ExecuteHostCommand(String::from("Nothing happens")),
    );

    // Create the Emacs edit mode with our custom keybindings
    let edit_mode = Box::new(Emacs::new(keybindings));

    // Create the line editor
    let mut line_editor = Reedline::create().with_edit_mode(edit_mode);

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                if buffer == "exit" {
                    println!("Goodbye!");
                    break;
                }
                println!("You typed: {buffer}");
            }
            Signal::CtrlD => {
                println!("\nQuitting.");
                break;
            }
            Signal::CtrlC => {
                println!("Ctrl+C pressed");
            }
        }
    }

    Ok(())
}
