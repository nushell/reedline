// Create a reedline object with automatic pairs.
// cargo run --example auto_pairs

use reedline::{AutoPairs, DefaultPrompt, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let auto_pairs = AutoPairs::new([('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]);
    let mut line_editor = Reedline::create().with_auto_pairs(auto_pairs);
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
