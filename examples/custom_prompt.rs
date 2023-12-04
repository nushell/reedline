// Create a reedline object with a custom prompt.
// cargo run --example custom_prompt
//
// Pressing keys will increase the right prompt value

use reedline::{
    Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, Reedline, Signal,
};
use std::{borrow::Cow, cell::Cell, io};

// For custom prompt, implement the Prompt trait
//
// This example displays the number of keystrokes
// or rather increments each time the prompt is rendered.
#[derive(Clone)]
pub struct CustomPrompt(Cell<u32>, &'static str);
pub static DEFAULT_MULTILINE_INDICATOR: &str = "::: ";
impl Prompt for CustomPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        {
            Cow::Owned(self.1.to_string())
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        {
            let old = self.0.get();
            self.0.set(old + 1);
            Cow::Owned(format!("[{old}]"))
        }
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<str> {
        Cow::Owned(">".to_string())
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed(DEFAULT_MULTILINE_INDICATOR)
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };

        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

fn main() -> io::Result<()> {
    println!("Custom prompt demo:\nAbort with Ctrl-C or Ctrl-D");
    let mut line_editor = Reedline::create();

    let prompt = CustomPrompt(Cell::new(0), "Custom Prompt");

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
