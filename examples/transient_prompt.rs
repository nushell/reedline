// Create a reedline object with a transient prompt.
// cargo run --example transient_prompt
//
// Prompts for previous lines will be replaced with a shorter prompt

#[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
use reedline::SqliteBackedHistory;
use reedline::{
    Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus, Reedline, Signal,
};
use std::{borrow::Cow, cell::Cell, io};

// For custom prompt, implement the Prompt trait
//
// This example replaces the prompt for old lines with "!" as an
// example of a transient prompt.
#[derive(Clone)]
pub struct TransientPrompt {
    /// Whether to show the transient prompt indicator instead of the normal one
    show_transient: Cell<bool>,
}
pub static DEFAULT_MULTILINE_INDICATOR: &str = "::: ";
pub static NORMAL_PROMPT: &str = "(transient_prompt example)";
pub static TRANSIENT_PROMPT: &str = "!";
impl Prompt for TransientPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        {
            if self.show_transient.get() {
                Cow::Owned(String::new())
            } else {
                Cow::Borrowed(NORMAL_PROMPT)
            }
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        Cow::Owned(String::new())
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<str> {
        if self.show_transient.get() {
            Cow::Borrowed(TRANSIENT_PROMPT)
        } else {
            Cow::Owned(">".to_string())
        }
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

    fn repaint_on_enter(&self) -> bool {
        // This method is called whenever the user hits enter to go to the next
        // line, so we want it to repaint and display the transient prompt
        self.show_transient.set(true);
        true
    }
}

fn main() -> io::Result<()> {
    println!("Transient prompt demo:\nAbort with Ctrl-C or Ctrl-D");
    let mut line_editor = Reedline::create();
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    {
        line_editor = line_editor.with_history(Box::new(SqliteBackedHistory::in_memory().unwrap()));
    }

    let prompt = TransientPrompt {
        show_transient: Cell::new(false),
    };

    loop {
        // We're on a new line, so make sure we're showing the normal prompt
        prompt.show_transient.set(false);
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
