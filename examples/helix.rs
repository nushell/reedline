// Create a reedline object with the experimental Helix edit mode.
// cargo run --example helix --features helix
//
// The current Helix example maps Ctrl-D to exit and uses the default prompt,
// which renders the active custom mode indicator as "(helix)".

use reedline::{DefaultPrompt, Helix, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    println!("Helix edit mode demo:\nAbort with Ctrl-C");

    let prompt = DefaultPrompt::default();
    let mut line_editor = Reedline::create().with_edit_mode(Box::new(Helix));

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
        }
    }
}
