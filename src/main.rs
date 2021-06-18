use crossterm::{event::KeyCode, event::KeyModifiers, Result};

use reedline::{
    default_emacs_keybindings, DefaultPrompt, EditCommand, EditMode, EmacsLineEditor, LineEditor,
    Signal, ViLineEditor,
};

fn main() -> Result<()> {
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Char('m'),
        vec![EditCommand::BackspaceWord],
    );

    let edit_mode = EditMode::Emacs;

    let mut line_editor: Box<dyn LineEditor> = match edit_mode {
        EditMode::Emacs => EmacsLineEditor::new(),
        EditMode::Vi => ViLineEditor::new(),
    };

    let prompt = Box::new(DefaultPrompt::new(1));

    // quick command like parameter handling
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        line_editor.print_line("Ready to print events:")?;
        line_editor.print_events()?;
        println!();
        return Ok(());
    };

    loop {
        let sig = line_editor.read_line(prompt.clone())?;

        match sig {
            Signal::CtrlD => {
                break;
            }
            Signal::Success(buffer) => {
                if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                    break;
                }
                if buffer.trim() == "clear" {
                    line_editor.clear_screen()?;
                    continue;
                }
                if buffer.trim() == "history" {
                    line_editor.print_history()?;
                    continue;
                }
                line_editor.print_line(&format!("Our buffer: {}", buffer))?;
            }
            Signal::CtrlC => {
                // We need to move one line down to start with the prompt on a new line
                line_editor.print_crlf()?;
            }
            Signal::CtrlL => {
                line_editor.clear_screen()?;
            }
        }
    }

    println!();
    Ok(())
}
