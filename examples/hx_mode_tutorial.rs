// Interactive tutorial for Helix keybinding mode
// cargo run --example hx_mode_tutorial

use reedline::{Helix, Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};
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
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => Cow::Borrowed("[ NORMAL ] 〉"),
                reedline::PromptViMode::Insert => Cow::Borrowed("[ INSERT ] : "),
                reedline::PromptViMode::Select => Cow::Borrowed("[ SELECT ] » "),
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
        Self {
            completed: false,
        }
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
            println!("Try the sandbox to experiment: cargo run --example hx_mode_sandbox\n");
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
    println!("Helix Mode Interactive Tutorial");
    println!("================================\n");
    println!("Welcome! Complete the full workflow in a single editing session.");
    println!("You'll transform 'hello world' into 'hello universe'.\n");

    println!("Quick reference:");
    println!("  Modes: NORMAL (commands) ⟷ INSERT (typing)");
    println!("  Exit: Ctrl+C or Ctrl+D at any time\n");

    let helix = Helix::default();
    let mut line_editor = Reedline::create().with_edit_mode(Box::new(helix));
    let prompt = HelixPrompt;
    let mut guide = TutorialGuide::new();

    // Show instructions
    guide.print_instructions();

    loop {
        let sig = line_editor.read_line(&prompt)?;

        match sig {
            Signal::Success(buffer) => {
                let success = guide.check_submission(&buffer);

                if guide.completed {
                    break Ok(());
                } else if !success {
                    println!("Not quite right. Expected 'hello universe' (without 'world').");
                    println!("Try again on the next prompt!\n");
                }
            }
            Signal::CtrlD | Signal::CtrlC => {
                if guide.completed {
                    println!("\nGoodbye! 👋");
                } else {
                    println!("\nTutorial interrupted. Run again to try once more!");
                }
                break Ok(());
            }
        }
    }
}
