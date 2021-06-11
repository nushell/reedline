use crossterm::Result;

use reedline::{
    DefaultPrompt, History, Reedline, Signal, DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR,
    HISTORY_SIZE,
};

fn main() -> Result<()> {
    let mut line_editor = match std::env::var("REEDLINE_HISTFILE") {
        Ok(histfile) if !histfile.is_empty() => {
            // TODO: Allow change of capacity and don't unwrap
            let history = History::with_file(HISTORY_SIZE, histfile.into()).unwrap();
            Reedline::with_history(history)
        }
        _ => Reedline::new(),
    };

    let prompt = DefaultPrompt::new(DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_INDICATOR, 1);

    // quick command like parameter handling
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        line_editor.print_line("Ready to print events:")?;
        line_editor.print_events()?;
        println!();
        return Ok(());
    };

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::CtrlD => {
                break;
            }
            Signal::Success(buffer) => {
                if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                    break;
                }
                if buffer.trim() == "clear" {
                    line_editor.clear_screen()?;
                    continue;
                }
                if buffer.trim() == "history" {
                    line_editor.print_history()?;
                    continue;
                }
                line_editor.print_line(&format!("Our buffer: {}", buffer))?;
            }
            Signal::CtrlC => {
                // We need to move one line down to start with the prompt on a new line
                line_editor.print_crlf()?;
            }
            Signal::CtrlL => {
                line_editor.clear_screen()?;
            }
        }
    }

    println!();
    Ok(())
}
