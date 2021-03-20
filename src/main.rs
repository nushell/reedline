use crossterm::{
    terminal::{self},
    Result,
};
use std::io::stdout;
mod line_buffer;

mod engine;
use engine::{clear_screen, print_crlf, print_message, Engine, Signal};

mod diagnostic;
use diagnostic::print_events;

mod history_search;

mod prompt;
use prompt::Prompt;

fn main() -> Result<()> {
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;
    // quick command like parameter handling
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        print_message(&mut stdout, "Ready to print events:")?;
        print_events(&mut stdout)?;
        terminal::disable_raw_mode()?;
        println!();
        return Ok(());
    };

    let mut engine = Engine::new();

    loop {
        if let Ok(sig) = engine.read_line(&mut stdout) {
            match sig {
                Signal::CtrlD => {
                    break;
                }
                Signal::Success(buffer) => {
                    if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                        break;
                    }
                    if buffer.trim() == "clear" {
                        clear_screen(&mut stdout)?;
                        continue;
                    }
                    if buffer.trim() == "history" {
                        engine.print_history(&mut stdout)?;
                        continue;
                    }
                    print_message(&mut stdout, &format!("Our buffer: {}", buffer))?;
                }
                Signal::CtrlC => {
                    // We need to move one line down to start with the prompt on a new line
                    print_crlf(&mut stdout)?;
                }
                Signal::CtrlL => {
                    clear_screen(&mut stdout)?;
                }
            }
        }
    }

    terminal::disable_raw_mode()?;

    println!();
    Ok(())
}
