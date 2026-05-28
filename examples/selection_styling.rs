use nu_ansi_term::{Color, Style};
use reedline::{DefaultPrompt, Reedline, Signal, Vi};
use std::io;

fn main() -> io::Result<()> {
    // `with_visual_selection_style` accepts any `nu_ansi_term::Style`.
    let selection_style = Style::new().on(Color::Blue).fg(Color::White);

    // Other shapes worth trying:
    // let selection_style = Style::new().reverse();
    // let selection_style = Style::new().fg(Color::Yellow).underline();
    // let selection_style = Style::new().on(Color::Rgb(60, 60, 90)).bold();

    let mut line_editor = Reedline::create()
        .with_visual_selection_style(selection_style)
        .with_edit_mode(Box::new(Vi::default()));

    let prompt = DefaultPrompt::default();

    println!("Type some text, press Esc to enter normal mode, then `v` for visual mode.");
    println!("Use h/j/k/l or arrow keys to extend the selection.\n");

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
