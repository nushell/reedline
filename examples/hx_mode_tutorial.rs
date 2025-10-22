// Helix mode interactive tutorial & sandbox
// Guided: cargo run --example hx_mode_tutorial
// Sandbox: cargo run --example hx_mode_tutorial -- --sandbox

use reedline::{
    Helix, Prompt, PromptEditMode, PromptHelixMode, PromptHistorySearch, Reedline, Signal,
};
use std::borrow::Cow;
use std::env;
use std::io;

#[derive(Clone, Copy)]
enum PromptStyle {
    Tutorial,
    Minimal,
}

struct HelixPrompt {
    style: PromptStyle,
}

impl HelixPrompt {
    fn new(style: PromptStyle) -> Self {
        Self { style }
    }

    fn set_style(&mut self, style: PromptStyle) {
        self.style = style;
    }
}

impl Prompt for HelixPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match (self.style, edit_mode) {
            (
                PromptStyle::Tutorial,
                PromptEditMode::Helix(helix_mode),
            ) => match helix_mode {
                PromptHelixMode::Normal => Cow::Borrowed("[ NORMAL ] 〉"),
                PromptHelixMode::Insert => Cow::Borrowed("[ INSERT ] : "),
                PromptHelixMode::Select => Cow::Borrowed("[ SELECT ] » "),
            },
            (
                PromptStyle::Minimal,
                PromptEditMode::Helix(helix_mode),
            ) => match helix_mode {
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

struct TutorialGuide {
    completed: bool,
}

impl TutorialGuide {
    fn new() -> Self {
        Self { completed: false }
    }

    fn check_submission(&mut self, buffer: &str) -> bool {
        if self.completed {
            return false;
        }

        // Check if they completed the full workflow
        if buffer.contains("hello") && buffer.contains("universe") && !buffer.contains("world") {
            println!("\n🎉 Tutorial Complete! 🎉");
            println!("═══════════════════════\n");
            println!("You successfully completed the basic workflow:");
            println!("  • Entered INSERT mode with 'i'");
            println!("  • Typed 'hello world'");
            println!("  • Returned to NORMAL mode with Esc");
            println!("  • Used motions (b, e) to select 'world'");
            println!("  • Deleted with 'd'");
            println!("  • Added 'universe' with 'i' + typing");
            println!("  • Submitted with Enter\n");
            println!("Perfect! Final result: {}\n", buffer);
            println!("You now understand the fundamentals of Helix mode!");
            println!("Stay in this session to experiment freely.");
            println!("Prompt will switch to sandbox mode (〉/:/» indicators).\n");
            self.completed = true;
            return true;
        }

        false
    }

    fn print_instructions(&self) {
        println!("\n╭─ Complete the Full Workflow ─────────────────────────────╮");
        println!("│  1. Press 'i' to enter INSERT mode                      │");
        println!("│  2. Type: hello world                                   │");
        println!("│  3. Press Esc to return to NORMAL mode                  │");
        println!("│  4. Press 'b' to move to start of 'world'               │");
        println!("│  5. Press 'e' to extend selection to end of 'world'     │");
        println!("│  6. Press 'd' to delete the selection                   │");
        println!("│  7. Press 'i' to enter INSERT mode again                │");
        println!("│  8. Type: universe                                      │");
        println!("│  9. Press Enter to submit                               │");
        println!("╰──────────────────────────────────────────────────────────╯");
        println!("💡 Goal: Transform 'hello world' → 'hello universe'");
        println!("💡 Watch the prompt change: [ NORMAL ] 〉 ⟷ [ INSERT ] :\n");
    }
}

fn main() -> io::Result<()> {
    let sandbox_requested = env::args().skip(1).any(|arg| arg == "--sandbox");

    if sandbox_requested {
        println!("Helix Mode Sandbox");
        println!("==================");
        println!("Prompt: 〉(normal)  :(insert)  »(select)");
        println!("Exit: Ctrl+C or Ctrl+D\n");
    } else {
        println!("Helix Mode Interactive Tutorial");
        println!("================================\n");
        println!("Welcome! Complete the full workflow in a single editing session.");
        println!("You'll transform 'hello world' into 'hello universe'.\n");

        println!("Quick reference:");
        println!("  Modes: NORMAL (commands) ⟷ INSERT (typing)");
        println!("  Exit: Ctrl+C or Ctrl+D at any time\n");
    }

    let helix = Helix::default();
    let mut line_editor = Reedline::create().with_edit_mode(Box::new(helix));
    let mut prompt = HelixPrompt::new(if sandbox_requested {
        PromptStyle::Minimal
    } else {
        PromptStyle::Tutorial
    });
    let mut guide = if sandbox_requested {
        None
    } else {
        Some(TutorialGuide::new())
    };
    let mut sandbox_active = sandbox_requested;
    let mut tutorial_completed = false;

    // Show instructions
    if let Some(guide_ref) = guide.as_ref() {
        guide_ref.print_instructions();
    }

    loop {
        let sig = line_editor.read_line(&prompt)?;

        match sig {
            Signal::Success(buffer) => {
                let mut needs_retry_message = false;
                let mut completed_now = false;

                if let Some(guide_ref) = guide.as_mut() {
                    let success = guide_ref.check_submission(&buffer);

                    if guide_ref.completed {
                        tutorial_completed = true;
                        completed_now = true;
                    } else if !success {
                        needs_retry_message = true;
                    }
                }

                if completed_now {
                    println!("Continue experimenting below or exit with Ctrl+C/D when finished.\n");
                    prompt.set_style(PromptStyle::Minimal);
                    guide = None;
                    sandbox_active = true;
                    continue;
                }

                if needs_retry_message {
                    println!("Not quite right. Expected 'hello universe' (without 'world').");
                    println!("Try again on the next prompt!\n");
                } else if sandbox_active {
                    println!("{buffer}");
                }
            }
            Signal::CtrlD | Signal::CtrlC => {
                if tutorial_completed || sandbox_active {
                    println!("\nGoodbye! 👋");
                } else {
                    println!("\nTutorial interrupted. Run again to try once more!");
                }
                break Ok(());
            }
        }
    }
}
