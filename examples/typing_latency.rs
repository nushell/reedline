//! Measure the typing latency of Reedline with default configurations.
//!
//! The result is not accurate on Windows, since alacritty has very bad latency
//! response on Windows due to the way it implemented async-IO without IOCP.

use alacritty_test::{pty_spawn, PtyExt, Terminal};
use reedline::{DefaultPrompt, Reedline, Signal};
use std::time::{Duration, Instant};

fn child() -> ! {
    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt).unwrap();
        match sig {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            _ => std::process::exit(-1),
        }
    }
}

fn main() -> std::io::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--child" {
            child();
        }
    }

    let mut pty = pty_spawn(
        "target/debug/examples/typing_latency",
        vec!["--child"],
        None,
    )?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    // Test latency of a single keystroke.
    let mut total_latency = Duration::from_millis(0);
    for loop_cnt in 1.. {
        let old_cursor = terminal.inner().grid().cursor.point;

        // input a single keystroke
        pty.write_all(b"A").unwrap();

        let start_time = Instant::now();
        loop {
            // measure with 10us accuracy
            terminal.read_from_pty(&mut pty, Some(Duration::from_micros(10)))?;

            let new_cursor = terminal.inner().grid().cursor.point;
            if new_cursor.column > old_cursor.column {
                break;
            }
        }
        let latency = start_time.elapsed();
        total_latency += latency;

        println!(
            "single keystroke latency = {:.2}ms, average latency = {:.2}ms over {loop_cnt} iterations",
            (latency.as_millis() as f64),
            (total_latency.as_millis() as f64) / (loop_cnt as f64)
        );

        // delete the keystroke
        pty.write_all(b"\x7f\x7f\x7f\x7f").unwrap();
        terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    }

    Ok(())
}
