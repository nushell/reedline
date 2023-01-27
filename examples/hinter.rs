// Create a reedline object with in-line hint support.
// cargo run --example hinter
//
// Fish-style history based hinting.
// assuming history ["abc", "ade"]
// pressing "a" hints to abc.
// Up/Down or Ctrl p/n, to select next/previous match

use nu_ansi_term::{Color, Style};
use reedline::{DefaultHinter, DefaultPrompt, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let mut line_editor = Reedline::create().with_hinter(Box::new(
        DefaultHinter::default().with_style(Style::new().italic().fg(Color::LightGray)),
    ));
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
