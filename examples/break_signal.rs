// Demonstrates the external break signal feature.
// A background thread sets the break signal after 3 seconds,
// causing `read_line()` to return `Signal::ExternalBreak` with
// the current buffer contents.
//
// To run:
// cargo run --example break_signal

use reedline::{DefaultPrompt, Reedline, Signal};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

fn main() -> io::Result<()> {
    let break_signal = Arc::new(AtomicBool::new(false));

    let mut line_editor = Reedline::create().with_break_signal(break_signal.clone());
    let prompt = DefaultPrompt::default();

    // Spawn a thread that triggers the break signal after 3 seconds
    let signal = break_signal.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(3));
        println!("\n[background] Setting break signal in...");
        signal.store(true, Ordering::Relaxed);
    });

    println!("Type something. The break signal will fire in 3 seconds...");

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
            Signal::ExternalBreak(buffer) => {
                println!("\nExternalBreak received! Buffer contents: {buffer:?}");
                break Ok(());
            }
            _ => {}
        }
    }
}
