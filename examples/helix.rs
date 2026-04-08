// Create a reedline object with the experimental Helix edit mode.
// cargo run --example helix --features helix
//
// The current Helix example maps Ctrl-D to exit and uses the default prompt,
// which renders the active custom mode indicator as "(helix)".

use reedline::{DefaultPrompt, Helix, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    println!(
        "Helix edit mode demo:
Default mode is insert (`:` prompt), so you can type words.
Press Esc for normal mode.
Press `i` to return to insert mode, or `a` to insert after the current selection.
Only `h`/`l` motions are currently implemented.
Abort with Ctrl-C"
    );

    let prompt = DefaultPrompt::default();
    let mut line_editor = Reedline::create().with_edit_mode(Box::new(Helix::default()));

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
