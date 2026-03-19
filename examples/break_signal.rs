// Demonstrates the external break signal feature.
//
// A background thread sets the break signal every 5 seconds,
// causing `read_line()` to return `Signal::ExternalBreak` with
// the current buffer contents. The example then resumes editing
// by calling `read_line()` again — the prompt stays on the same
// line thanks to suspended-state preservation.
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

    // Spawn a thread that triggers the break signal periodically
    let signal = break_signal.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(5));
        signal.store(true, Ordering::Relaxed);
    });

    println!("Type something. The break signal fires every 5 seconds.");
    println!("The prompt will stay in place after each ExternalBreak.\n");

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
                // The buffer contents are preserved across the break.
                // Simply call read_line() again to let the user continue
                // editing — the prompt stays on the same line as long as
                // nothing is printed between the break and the next
                // read_line() call.
                eprintln!("[break] buffer: {buffer:?}");
                continue;
            }
            _ => {}
        }
    }
}
