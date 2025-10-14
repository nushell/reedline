// Minimal Helix mode sandbox for experimentation
// cargo run --example hx_mode_sandbox

use reedline::{
    Helix, Prompt, PromptEditMode, PromptHelixMode, PromptHistorySearch, Reedline, Signal,
};
use std::borrow::Cow;
use std::io;

struct HelixPrompt;

impl Prompt for HelixPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Helix(helix_mode) => match helix_mode {
                PromptHelixMode::Normal => Cow::Borrowed("〉"),
                PromptHelixMode::Insert => Cow::Borrowed(": "),
                PromptHelixMode::Select => Cow::Borrowed("» "),
            },
            _ => Cow::Borrowed("> "),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("::: ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        let prefix = match history_search.status {
            reedline::PromptHistorySearchStatus::Passing => "",
            reedline::PromptHistorySearchStatus::Failing => "failing ",
        };
        Cow::Owned(format!(
            "({}reverse-search: {}) ",
            prefix, history_search.term
        ))
    }
}

fn main() -> io::Result<()> {
    println!("Helix Mode Sandbox");
    println!("==================");
    println!("Prompt: 〉(normal)  :(insert)  »(select)");
    println!("Exit: Ctrl+C or Ctrl+D\n");

    let mut line_editor = Reedline::create().with_edit_mode(Box::new(Helix::default()));
    let prompt = HelixPrompt;

    loop {
        let sig = line_editor.read_line(&prompt)?;

        match sig {
            Signal::Success(buffer) => {
                println!("{buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                break Ok(());
            }
        }
    }
}
