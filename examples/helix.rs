// Create a reedline object with the full experimental Helix edit mode.
// cargo run --example helix --features helix
//
// This example uses the public Helix mode exported by the crate, including
// Select mode and the extended normal-mode command set. The default prompt
// renders the active Helix mode indicator.

use reedline::{DefaultPrompt, Helix, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    println!("Helix edit mode demo:\nAbort with Ctrl-C");

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
