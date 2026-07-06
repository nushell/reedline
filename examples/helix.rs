use nu_ansi_term::Style;
// Create a reedline object with the experimental Helix edit mode.
// cargo run --example helix --features helix
use reedline::{DefaultPrompt, Helix, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let prompt = DefaultPrompt::default();
    let selection_style = Style::new().reverse();
    let mut line_editor = Reedline::create()
        .with_edit_mode(Box::new(Helix::default()))
        .with_visual_selection_style(selection_style);

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
