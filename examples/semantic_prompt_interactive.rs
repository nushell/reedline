// Semantic prompt interactive example
// Run with: cargo run --example semantic_prompt_interactive
//
// This example demonstrates OSC 133 semantic prompt markers.
// Use this with Ghostty's "Overlay Semantic Prompts" debug feature
// to visually verify marker placement.
//
// The expected sequence on screen is:
//   [A;k=i][left prompt][indicator][B][user input area][P;k=r][right prompt]
//
// Where:
//   A;k=i = Start of primary prompt (interactive)
//   B     = Start of user input area (end of prompt)
//   P;k=r = Right prompt marker

use reedline::{DefaultPrompt, Osc133Markers, Reedline, Signal};
use std::io;

fn main() -> io::Result<()> {
    println!("Semantic Prompt Interactive Demo");
    println!("=================================");
    println!("This demo uses OSC 133 markers for semantic prompts.");
    println!("If you're using Ghostty, enable 'Overlay Semantic Prompts' in the debug menu.");
    println!("You should see colored overlays indicating prompt regions.");
    println!();
    println!("Type commands and press Enter. Type 'exit' or press Ctrl+D to quit.");
    println!();

    // Create a Reedline instance with semantic markers enabled
    let mut line_editor = Reedline::create().with_semantic_markers(Some(Osc133Markers::boxed()));

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                let buffer = buffer.trim();
                if buffer == "exit" {
                    println!("Goodbye!");
                    break Ok(());
                }
                println!("You entered: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
        }
    }
}
