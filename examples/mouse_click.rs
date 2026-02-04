// Enable mouse click-to-cursor support.
// cargo run --example mouse_click

use reedline::{DefaultPrompt, MouseClickMode, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let mut line_editor = Reedline::create().with_mouse_click(MouseClickMode::EnabledWithOsc133);
    let prompt = DefaultPrompt::default();

    println!("Mouse click-to-cursor enabled.");
    println!("Type some text, then click in the line to move the cursor.");
    println!("Press Enter to submit, Ctrl-D/Ctrl-C to exit.");

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
