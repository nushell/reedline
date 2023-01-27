// Create a reedline object with a custom validator to break the line on unfinished input.
// cargo run --example validator
//
// Input "complete" followed by [Enter], will accept the input line (Signal::Succeed will be called)
// Pressing [Enter] will in other cases give you a multi-line prompt.

use reedline::{DefaultPrompt, Reedline, Signal, ValidationResult, Validator};
use std::io;

struct CustomValidator;

// For custom validation, implement the Validator trait
impl Validator for CustomValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        if line == "complete" {
            ValidationResult::Complete
        } else {
            ValidationResult::Incomplete
        }
    }
}

fn main() -> io::Result<()> {
    println!("Input \"complete\" followed by [Enter], will accept the input line (Signal::Succeed will be called)\nPressing [Enter] will in other cases give you a multi-line prompt.\nAbort with Ctrl-C or Ctrl-D");
    let mut line_editor = Reedline::create().with_validator(Box::new(CustomValidator));

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
