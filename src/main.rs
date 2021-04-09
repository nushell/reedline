use crossterm::Result;

use reedline::{Engine, Signal};

fn main() -> Result<()> {
    let mut engine = Engine::new();

    // quick command like parameter handling
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        engine.print_message("Ready to print events:")?;
        engine.print_events()?;
        println!();
        return Ok(());
    };

    loop {
        if let Ok(sig) = engine.read_line() {
            match sig {
                Signal::CtrlD => {
                    break;
                }
                Signal::Success(buffer) => {
                    if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                        break;
                    }
                    if buffer.trim() == "clear" {
                        engine.clear_screen()?;
                        continue;
                    }
                    if buffer.trim() == "history" {
                        engine.print_history()?;
                        continue;
                    }
                    engine.print_message(&format!("Our buffer: {}", buffer))?;
                }
                Signal::CtrlC => {
                    // We need to move one line down to start with the prompt on a new line
                    engine.print_crlf()?;
                }
                Signal::CtrlL => {
                    engine.clear_screen()?;
                }
            }
        }
    }

    println!();
    Ok(())
}
