// Create a default reedline object to handle user input
// cargo run --example abbreviations
//
// You can browse the local (non persistent) history using Up/Down or Ctrl n/p.

use reedline::{DefaultPrompt, Reedline, Signal};
use std::{collections::HashMap, io};

fn main() -> io::Result<()> {
    let mut abbrevs = HashMap::new();
    abbrevs.insert(String::from("ll"), String::from("ls -l"));
    abbrevs.insert(String::from("gs"), String::from("git status"));
    // Create a new Reedline engine with a local History that is not synchronized to a file and our
    // abbreviations
    let mut line_editor = Reedline::create().with_abbreviations(abbrevs);
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
