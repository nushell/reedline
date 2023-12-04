// Create a reedline object with highlighter support.
// cargo run --example highlighter
//
// unmatched input is marked red, matched input is marked green
use reedline::{DefaultPrompt, ExampleHighlighter, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let commands = vec![
        "test".into(),
        "hello world".into(),
        "hello world reedline".into(),
        "this is the reedline crate".into(),
    ];
    let mut line_editor =
        Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
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
