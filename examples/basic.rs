// Create a default reedline object to handle user input
// cargo run --example basic
//
// You can browse the local (non persistent) history using Up/Down or Ctrl n/p.

use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    // Create a new Reedline engine with a local History that is not synchronized to a file.
    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::WorkingDirectory,
        DefaultPromptSegment::Dynamic(|| {
            jiff::Zoned::now()
                .strftime("%m/%d/%Y %I:%M:%S %p")
                .to_string()
        }),
    );

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
