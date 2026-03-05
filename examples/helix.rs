// Interactive Helix edit mode sandbox.
// cargo run --features=hx --example helix

use reedline::{CursorConfig, DefaultPrompt, Helix, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    let mut line_editor = Reedline::create()
        .with_edit_mode(Box::new(Helix::default()))
        .with_cursor_config(CursorConfig::with_hx_defaults());
    let prompt = DefaultPrompt::default();

    println!("Helix edit mode demo. Starts in Normal mode.");
    println!("  i = Insert, Esc = Normal, v = Select");
    println!("  h/l = left/right, w/b/e = word motions");
    println!("  Ctrl+C or Ctrl+D to exit\n");

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
