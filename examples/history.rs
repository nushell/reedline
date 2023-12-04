// Create a reedline object with history support, including history size limits.
// cargo run --example history
//
// A file `history.txt` will be created (or replaced).
// Allows for persistent loading of previous session.
//
// Browse history by Up/Down arrows or Ctrl-n/p

use reedline::{DefaultPrompt, FileBackedHistory, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let history = Box::new(
        FileBackedHistory::with_file(5, "history.txt".into())
            .expect("Error configuring history with file"),
    );

    let mut line_editor = Reedline::create().with_history(history);
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
        }
    }
}
