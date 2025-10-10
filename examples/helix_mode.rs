// Example demonstrating Helix keybinding mode
// cargo run --example helix_mode
// cargo run --example helix_mode -- --simple-prompt
//
// Shows Helix-style modal editing with configurable prompt

use reedline::{Helix, Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};
use std::borrow::Cow;
use std::env;
use std::io;

// Prompt with explicit mode display
struct HelixModePrompt;

impl Prompt for HelixModePrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => Cow::Borrowed("[ NORMAL ] 〉"),
                reedline::PromptViMode::Insert => Cow::Borrowed("[ INSERT ] : "),
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

// Simple prompt with icon-only mode indicators
struct SimplePrompt;

impl Prompt for SimplePrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => Cow::Borrowed("〉"),
                reedline::PromptViMode::Insert => Cow::Borrowed(": "),
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
    let args: Vec<String> = env::args().collect();
    let simple_prompt = args.iter().any(|arg| arg == "--simple-prompt");

    println!("Helix Mode Demo");
    println!("===============");
    println!("Starting in NORMAL mode");
    println!();

    if simple_prompt {
        println!("Using simple icon prompt:");
        println!("  〉 (normal mode)");
        println!("  :  (insert mode)");
    } else {
        println!("Using explicit mode display:");
        println!("  [ NORMAL ] 〉 (default)");
        println!("  [ INSERT ] :  (after pressing i/a/I/A)");
        println!();
        println!("Tip: Use --simple-prompt for icon-only indicators");
    }

    println!();
    println!("Keybindings:");
    println!("  Insert: i/a/I/A         Motions: h/l/w/b/e/0/$");
    println!("  Select: x ; Alt+;       Edit: d/c/y/p/P");
    println!("  Exit: Esc/Ctrl+C/Ctrl+D");
    println!();
    println!("Note: Motions extend selection (Helix-style)");
    println!();

    let mut line_editor = Reedline::create().with_edit_mode(Box::new(Helix::default()));

    if simple_prompt {
        let prompt = SimplePrompt;
        loop {
            let sig = line_editor.read_line(&prompt)?;
            match sig {
                Signal::Success(buffer) => {
                    println!("You entered: {buffer}");
                }
                Signal::CtrlD | Signal::CtrlC => {
                    println!("\nExiting!");
                    break Ok(());
                }
            }
        }
    } else {
        let prompt = HelixModePrompt;
        loop {
            let sig = line_editor.read_line(&prompt)?;
            match sig {
                Signal::Success(buffer) => {
                    println!("You entered: {buffer}");
                }
                Signal::CtrlD | Signal::CtrlC => {
                    println!("\nExiting!");
                    break Ok(());
                }
            }
        }
    }
}
