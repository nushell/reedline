// Helix mode interactive tutorial & sandbox
// Guided: cargo run --example hx_mode_tutorial
// Sandbox: cargo run --example hx_mode_tutorial -- --sandbox

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use reedline::{
    EditCommand, EditMode, Helix, Prompt, PromptEditMode, PromptHelixMode, PromptHistorySearch,
    Reedline, ReedlineEvent, ReedlineRawEvent, Signal,
};
use std::borrow::Cow;
use std::env;
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum PromptStyle {
    Tutorial,
    Minimal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TutorialStage {
    NormalWorkflow,
    SelectMode,
    Completed,
}

enum SubmissionOutcome {
    Retry,
    Continue,
    Completed,
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
            (PromptStyle::Tutorial, PromptEditMode::Helix(helix_mode)) => match helix_mode {
                PromptHelixMode::Normal => Cow::Borrowed("[ NORMAL ] 〉"),
                PromptHelixMode::Insert => Cow::Borrowed("[ INSERT ] : "),
                PromptHelixMode::Select => Cow::Borrowed("[ SELECT ] » "),
            },
            (PromptStyle::Minimal, PromptEditMode::Helix(helix_mode)) => match helix_mode {
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

struct SharedHelix {
    state: Arc<Mutex<Helix>>,
}

impl SharedHelix {
    fn new(state: Arc<Mutex<Helix>>) -> Self {
        Self { state }
    }
}

impl EditMode for SharedHelix {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        let mut helix = self.state.lock().expect("helix lock poisoned");
        <Helix as EditMode>::parse_event(&mut *helix, event)
    }

    fn edit_mode(&self) -> PromptEditMode {
        let helix = self.state.lock().expect("helix lock poisoned");
        <Helix as EditMode>::edit_mode(&*helix)
    }
}

struct TutorialGuide {
    stage: TutorialStage,
}

impl TutorialGuide {
    fn new() -> Self {
        Self {
            stage: TutorialStage::NormalWorkflow,
        }
    }

    fn handle_submission(&mut self, buffer: &str) -> SubmissionOutcome {
        match self.stage {
            TutorialStage::NormalWorkflow => {
                if buffer.contains("hello")
                    && buffer.contains("universe")
                    && !buffer.contains("world")
                {
                    println!("\n🎉 Stage 1 Complete! 🎉");
                    println!("You mastered the normal-mode workflow:");
                    println!("  • Entered INSERT mode with 'i' (insert)");
                    println!("  • Typed 'hello world'");
                    println!("  • Stayed in INSERT mode when finishing the edit");
                    println!("  • Used 'b' (back) twice to land on the start of 'hello'");
                    println!("  • Highlighted 'hello' with 'e' (end of word) then saw 'w' (word) land in the gap ahead");
                    println!("  • Used 'w' (word) again to select 'world' and deleted using 'd' (delete)");
                    println!("  • Added 'universe' with 'i' (insert) + typing\n");
                    println!(
                        "Next up: practise Helix Select mode to edit with a highlighted region."
                    );
                    println!("We'll reset the buffer to 'hello universe' so you can inspect it before continuing.");
                    self.stage = TutorialStage::SelectMode;
                    self.print_current_stage_instructions();
                    SubmissionOutcome::Continue
                } else {
                    println!("Not quite right. Expected 'hello universe' (without 'world').");
                    println!("Follow the checklist and submit again.\n");
                    SubmissionOutcome::Retry
                }
            }
            TutorialStage::SelectMode => {
                if buffer.trim() == "goodbye friend" {
                    println!("\n🌟 Stage 2 Complete! 🌟");
                    println!("You performed a Select mode edit:");
                    println!("  • Entered Select mode with 'v' (visual/select)");
                    println!("  • Pressed 'b' (back) twice to highlight both words");
                    println!("  • Replaced the selection with 'c' (change) → 'goodbye friend'");
                    println!("  • Submitted directly from INSERT mode\n");
                    println!("Final result: {}\n", buffer);
                    println!("Tutorial accomplished! The prompt now switches to sandbox mode so you can explore.");
                    self.stage = TutorialStage::Completed;
                    SubmissionOutcome::Completed
                } else {
                    println!("Select mode step not finished. Goal: transform 'hello universe' → 'goodbye friend'.");
                    println!("Hint: enter Select mode with 'v' (visual/select), press 'b' (back) twice to grow the highlight, then 'c' (change) to replace it.\n");
                    SubmissionOutcome::Retry
                }
            }
            TutorialStage::Completed => SubmissionOutcome::Completed,
        }
    }

    fn stage(&self) -> TutorialStage {
        self.stage
    }

    fn print_current_stage_instructions(&self) {
        match self.stage {
            TutorialStage::NormalWorkflow => {
                println!("\n╭─ Stage 1: Normal Mode Workflow ──────────────────────────╮");
                println!("│  1. Press 'i' (insert) to enter INSERT mode             │");
                println!("│  2. Type: hello world                                   │");
                println!("│  3. Press 'b' (back) twice to land on the start of 'hello' │");
                println!(
                    "│  4. Press 'e' (end of word) to highlight 'hello' with the cursor on 'o'│"
                );
                println!("│  5. Press 'b' (back) to re-highlight 'hello', then 'w' (word) to land in the gap │");
                println!("│  6. Press 'w' (word) again to select 'world'            │");
                println!("│  7. Press 'd' (delete) to remove the word               │");
                println!("│  8. Press 'i' (insert) and type: universe               │");
                println!("│  9. Press Enter (submit) to finish                      │");
                println!("╰──────────────────────────────────────────────────────────╯");
                println!("💡 Goal: Transform 'hello world' → 'hello universe'");
                println!(
                    "💡 'e' highlights through the word end; 'w' settles in the gap before the next word."
                );
                println!("💡 Watch the prompt change: [ NORMAL ] 〉 ⟷ [ INSERT ] :\n");
            }
            TutorialStage::SelectMode => {
                println!("\n╭─ Stage 2: Select Mode Edit ──────────────────────────────╮");
                println!("│  1. Press 'v' (visual/select) to enter SELECT mode ([ SELECT ] ») │");
                println!("│  2. Press 'b' (back) to highlight the word 'universe'   │");
                println!("│  3. Press 'b' (back) again to include 'hello' in the highlight │");
                println!("│  4. Press 'c' (change) and type: goodbye friend         │");
                println!("│  5. Press Enter (submit) to finish                      │");
                println!("╰──────────────────────────────────────────────────────────╯");
                println!("💡 Goal: Transform 'hello universe' → 'goodbye friend'");
                println!("💡 You're already in NORMAL mode with 'hello universe' visible—hit 'v' to begin.");
                println!(
                    "💡 Notice how pressing 'b' in Select mode grows the highlight backward.\n"
                );
            }
            TutorialStage::Completed => {}
        }
    }
}

fn preload_stage_two_buffer(line_editor: &mut Reedline, helix_state: &Arc<Mutex<Helix>>) {
    ensure_stage_two_normal_mode(line_editor, helix_state);
    line_editor.run_edit_commands(&[EditCommand::ClearSelection]);
    line_editor.run_edit_commands(&[EditCommand::Clear]);
    line_editor.run_edit_commands(&[EditCommand::InsertString("hello universe".to_string())]);
    line_editor.run_edit_commands(&[EditCommand::MoveToEnd { select: false }]);
}

fn ensure_stage_two_normal_mode(line_editor: &mut Reedline, helix_state: &Arc<Mutex<Helix>>) {
    if let Ok(raw) =
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
    {
        let event = {
            let mut helix = helix_state.lock().expect("helix lock poisoned");
            <Helix as EditMode>::parse_event(&mut *helix, raw)
        };
        apply_reedline_event(line_editor, event);
    }
}

fn apply_reedline_event(line_editor: &mut Reedline, event: ReedlineEvent) {
    match event {
        ReedlineEvent::Edit(commands) => line_editor.run_edit_commands(&commands),
        ReedlineEvent::Multiple(events) => {
            for nested in events {
                apply_reedline_event(line_editor, nested);
            }
        }
        ReedlineEvent::Repaint | ReedlineEvent::Esc | ReedlineEvent::None => {}
        // The tutorial reset path only expects edit/esc/repaint events. Surface any new ones
        // during development so we do not silently drop behaviour updates from Helix.
        unexpected => {
            debug_assert!(
                false,
                "Unhandled ReedlineEvent during tutorial reset: {unexpected:?}"
            );
        }
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
        println!("Stage 1 covers normal-mode editing, Stage 2 introduces Select mode.\n");

        println!("Quick reference:");
        println!("  Modes: NORMAL (commands) ⟷ INSERT (typing)");
        println!("  Select mode: enter with 'v', exit with Esc");
        println!("  Exit: Ctrl+C or Ctrl+D at any time\n");
    }

    let helix_state = Arc::new(Mutex::new(Helix::default()));
    let mut line_editor =
        Reedline::create().with_edit_mode(Box::new(SharedHelix::new(helix_state.clone())));
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
        guide_ref.print_current_stage_instructions();
    }

    loop {
        let sig = line_editor.read_line(&prompt)?;

        match sig {
            Signal::Success(buffer) => {
                if let Some(guide_ref) = guide.as_mut() {
                    match guide_ref.handle_submission(&buffer) {
                        SubmissionOutcome::Retry => {}
                        SubmissionOutcome::Continue => {
                            if guide_ref.stage() == TutorialStage::SelectMode {
                                preload_stage_two_buffer(&mut line_editor, &helix_state);
                            }
                            continue;
                        }
                        SubmissionOutcome::Completed => {
                            tutorial_completed = true;
                            println!("Continue experimenting below or exit with Ctrl+C/D when finished.\n");
                            prompt.set_style(PromptStyle::Minimal);
                            guide = None;
                            sandbox_active = true;
                            continue;
                        }
                    }
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
