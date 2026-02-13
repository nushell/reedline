// Helix mode interactive tutorial and sandbox
//
// Guided tutorial:
//   cargo run --example helix_mode
//
// Jump to a chapter:
//   cargo run --example helix_mode -- --chapter 3
//
// Sandbox (free-form experimentation):
//   cargo run --example helix_mode -- --sandbox
//
// Unit tests live in src/edit_mode/helix/mod.rs:
//   cargo test helix

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use reedline::{
    EditCommand, EditMode, Helix, Prompt, PromptEditMode, PromptHelixMode, PromptHistorySearch,
    Reedline, ReedlineEvent, ReedlineRawEvent, Signal,
};
use std::borrow::Cow;
use std::env;
use std::io;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

struct HelixPrompt {
    /// When true, show full mode labels; when false, show minimal indicators.
    verbose: bool,
}

impl Prompt for HelixPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Helix(mode) if self.verbose => match mode {
                PromptHelixMode::Normal => Cow::Borrowed("NOR > "),
                PromptHelixMode::Insert => Cow::Borrowed("INS > "),
                PromptHelixMode::Select => Cow::Borrowed("SEL > "),
            },
            PromptEditMode::Helix(mode) => match mode {
                PromptHelixMode::Normal => Cow::Borrowed("> "),
                PromptHelixMode::Insert => Cow::Borrowed(": "),
                PromptHelixMode::Select => Cow::Borrowed("v "),
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

// ---------------------------------------------------------------------------
// Shared Helix edit-mode wrapper (allows the tutorial to inspect state)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Lesson definitions
// ---------------------------------------------------------------------------

struct Lesson {
    title: &'static str,
    body: &'static str,
    /// Text pre-loaded into the line buffer before the exercise.  When empty
    /// the buffer starts blank and the user types from scratch.
    preload: &'static str,
    /// Predicate: return `true` when the submitted buffer is correct.
    check: fn(&str) -> bool,
    /// Hint shown on incorrect submission.
    hint: &'static str,
}

fn lessons() -> Vec<Lesson> {
    vec![
        // =============================================================
        //  CHAPTER 1 - BASICS
        // =============================================================
        Lesson {
            title: "1.1 - INSERT MODE",
            body: "\
  Helix starts in Normal mode — keys execute commands, not text.
  Press i to enter Insert mode (prompt shows INS).
  Press Escape to return to Normal mode.

  Exercise: type i, type \"hello world\", then Enter.",
            preload: "",
            check: |buf| buf == "hello world",
            hint: "Type i, then hello world, then Enter.",
        },
        Lesson {
            title: "1.2 - MOVEMENT & DELETION",
            body: "\
  h / l  move left / right      d  delete selection

  In Normal mode the cursor is always a one-character selection,
  so d deletes the character under the cursor.

  Exercise: remove the doubled letters.
  Goal: \"This sentence has extra characters.\"",
            preload: "Thhis senttencee haass exxtra charracterss.",
            check: |buf| buf == "This sentence has extra characters.",
            hint: "Move with h/l to each doubled letter, press d to delete it.",
        },
        // =============================================================
        //  CHAPTER 2 - INSERT VARIANTS
        // =============================================================
        Lesson {
            title: "2.1 - INSERT VARIANTS",
            body: "\
  i  insert before cursor      I  insert at line start
  a  append after cursor       A  append at line end

  Exercise: buffer says \"world\".  Press I, type \"hello \", Esc, Enter.
  Goal: \"hello world\"",
            preload: "world",
            check: |buf| buf == "hello world",
            hint: "Press I (shift-i) to insert at line start, type hello (with trailing space), Escape, Enter.",
        },
        Lesson {
            title: "2.2 - APPENDING",
            body: "\
  Exercise: buffer says \"hello\".  Press A, type \" world\", Esc, Enter.
  Goal: \"hello world\"",
            preload: "hello",
            check: |buf| buf == "hello world",
            hint: "Press A (shift-a) to append at line end, type  world, Escape, Enter.",
        },
        // =============================================================
        //  CHAPTER 3 - MOTIONS AND SELECTIONS
        // =============================================================
        Lesson {
            title: "3.1 - WORD MOTIONS",
            body: "\
  w  select to next word start    W  same, whitespace-delimited
  e  select to word end           E  same, whitespace-delimited
  b  select to word start (back)  B  same, whitespace-delimited

  Exercise: delete the junk words with w then d.
  Goal: \"This sentence has extra words in it.\"",
            preload: "This sentence pencil has vacuum extra words in the it.",
            check: |buf| buf == "This sentence has extra words in it.",
            hint: "Move to a junk word, w to select it, d to delete.",
        },
        Lesson {
            title: "3.2 - CHANGE",
            body: "\
  c  delete selection and enter Insert mode (\"change\").

  Exercise: fix the wrong words.
  Goal: \"This sentence has incorrect words in it.\"
  Hint: e to select a word, c to change it.",
            preload: "This paper has heavy words behind it.",
            check: |buf| buf == "This sentence has incorrect words in it.",
            hint: "Select each wrong word with e, press c, type the correct word, Escape, repeat.",
        },
        Lesson {
            title: "3.3 - SELECT MODE & LINE SELECTION",
            body: "\
  v  toggle Select mode (motions extend instead of replace)
  x  select entire line
  ;  collapse selection to cursor    Alt-;  flip selection

  Exercise: remove \"FOO BAR \" and \"BAZ BIZ \" from the line.
  Hint: move to F, v, w w to extend, d to delete.  Repeat.
  Goal: \"Remove the distracting words from this line.\"",
            preload: "Remove the FOO BAR distracting words BAZ BIZ from this line.",
            check: |buf| buf == "Remove the distracting words from this line.",
            hint: "v to enter Select mode, w to extend, d to delete.",
        },
        // =============================================================
        //  CHAPTER 4 - UNDO, YANK, PASTE
        // =============================================================
        Lesson {
            title: "4.1 - UNDO / REDO",
            body: "\
  u  undo    U  redo

  Exercise: delete some characters, then u to restore.
  Goal: \"Fix the errors and use undo.\"",
            preload: "Fix the errors and use undo.",
            check: |buf| buf == "Fix the errors and use undo.",
            hint: "Delete something with d, then press u to undo, then Enter.",
        },
        Lesson {
            title: "4.2 - YANK & PASTE",
            body: "\
  y  yank (copy) selection    p  paste after    P  paste before

  Exercise: yank \"banana\" and paste it into the gaps.
  Goal: \"1 banana 2 banana 3 banana 4\"
  Hint: e on \"banana\", y to yank, move to gap, p to paste.",
            preload: "1 banana 2 3 4",
            check: |buf| buf == "1 banana 2 banana 3 banana 4",
            hint: "Select banana with e, yank with y, move to correct position, paste with p.",
        },
        // =============================================================
        //  CHAPTER 5 - FINDING & GOTO
        // =============================================================
        Lesson {
            title: "5.1 - FIND / TILL",
            body: "\
  f<ch>  select forward to <ch> (inclusive)
  t<ch>  select forward to <ch> (exclusive)
  F/T    same, backwards

  Exercise: remove the dashes and brackets.
  Goal: \"Free this sentence!\"
  Hint: f[ then d, then F] then d, then clean up with t/T.",
            preload: "-----[Free this sentence!]-----",
            check: |buf| buf == "Free this sentence!",
            hint: "Use f/t to select forward to a character, F/T to select backward.",
        },
        Lesson {
            title: "5.2 - GOTO",
            body: "\
  gh  goto line start    gl  goto line end    gs  first non-blank
  (0 and $ also work for start/end.)

  Exercise: try gh and gl, then Enter.",
            preload: "Jump to the start and end of this line.",
            check: |buf| buf == "Jump to the start and end of this line.",
            hint: "Just press Enter when you are done exploring.",
        },
        // =============================================================
        //  CHAPTER 6 - PUTTING IT ALL TOGETHER
        // =============================================================
        Lesson {
            title: "6.1 - COMPLETE WORKFLOW",
            body: "\
  Exercise: change \"world\" to \"universe\".
  Goal: \"hello universe\"",
            preload: "hello world",
            check: |buf| buf == "hello universe",
            hint: "Select world with e or w, press c, type universe, Escape, Enter.",
        },
        Lesson {
            title: "6.2 - SELECT MODE WORKFLOW",
            body: "\
  Exercise: replace the entire buffer.
  Goal: \"goodbye friend\"
  Hint: v, b b to select all, c to change.",
            preload: "hello universe",
            check: |buf| buf == "goodbye friend",
            hint: "v to enter Select mode, b b to select both words, c to change.",
        },
        Lesson {
            title: "TUTORIAL COMPLETE",
            body: "\
  Congratulations!  Quick reference:

  Movement       Editing        Modes
  --------       -------        -----
  h / l          d  delete      i / a  insert
  w / e / b      c  change      I / A  insert at line start/end
  W / E / B      y  yank        v      select mode
  f / t / F / T  p / P  paste   Esc    back to normal
  gh / gl / gs   u / U  undo
  0 / $          x  select line
                 ;  collapse    Ctrl-C / Ctrl-D  exit

  Entering sandbox mode.  Press Enter to continue or Ctrl-C to exit.",
            preload: "",
            check: |_| true,
            hint: "",
        },
    ]
}

// ---------------------------------------------------------------------------
// Tutorial runner
// ---------------------------------------------------------------------------

fn print_separator() {
    println!(
        "{}",
        "=".repeat(65)
    );
}

fn print_lesson(index: usize, total: usize, lesson: &Lesson) {
    println!();
    print_separator();
    println!("  {} ({}/{})", lesson.title, index + 1, total);
    print_separator();
    println!();
    println!("{}", lesson.body);
    println!();
}

fn preload_buffer(line_editor: &mut Reedline, helix_state: &Arc<Mutex<Helix>>, text: &str) {
    // Ensure we are in Normal mode before manipulating the buffer.
    force_normal_mode(line_editor, helix_state);
    line_editor.run_edit_commands(&[EditCommand::ClearSelection]);
    line_editor.run_edit_commands(&[EditCommand::Clear]);
    if !text.is_empty() {
        line_editor.run_edit_commands(&[EditCommand::InsertString(text.to_string())]);
        line_editor.run_edit_commands(&[EditCommand::MoveToEnd { select: false }]);
    }
}

fn force_normal_mode(line_editor: &mut Reedline, helix_state: &Arc<Mutex<Helix>>) {
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
        unexpected => {
            debug_assert!(
                false,
                "Unhandled ReedlineEvent during tutorial reset: {unexpected:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

/// Parse `--chapter N` from CLI args.  Returns `None` if not specified.
fn parse_chapter_arg() -> Option<usize> {
    let args: Vec<String> = env::args().collect();
    args.iter()
        .position(|a| a == "--chapter")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

/// Return the lesson index where the given chapter number starts.
fn chapter_start_index(lessons: &[Lesson], chapter: usize) -> Option<usize> {
    let prefix = format!("{chapter}.");
    lessons.iter().position(|l| l.title.contains(&prefix))
}

fn main() -> io::Result<()> {
    let sandbox = env::args().skip(1).any(|arg| arg == "--sandbox");

    let helix_state = Arc::new(Mutex::new(Helix::default()));
    let mut line_editor =
        Reedline::create().with_edit_mode(Box::new(SharedHelix::new(helix_state.clone())));
    let mut prompt = HelixPrompt { verbose: !sandbox };

    if sandbox {
        println!("Helix Mode Sandbox");
        println!("==================");
        println!("Modes:  > (normal)   : (insert)   v (select)");
        println!("Exit:   Ctrl-C / Ctrl-D\n");

        loop {
            match line_editor.read_line(&prompt)? {
                Signal::Success(buf) => println!("{buf}"),
                Signal::CtrlC | Signal::CtrlD => {
                    println!("\nGoodbye!");
                    break Ok(());
                }
            }
        }
    } else {
        // ---- Tutorial mode ----
        let all_lessons = lessons();
        let total = all_lessons.len();

        // --chapter N: jump to a specific chapter
        let mut index = match parse_chapter_arg() {
            Some(ch) => match chapter_start_index(&all_lessons, ch) {
                Some(i) => i,
                None => {
                    println!("Unknown chapter {ch}.  Available chapters:");
                    let mut seen = std::collections::BTreeSet::new();
                    for l in &all_lessons {
                        if let Some(dot) = l.title.find('.') {
                            let num = &l.title[..dot];
                            if let Ok(n) = num.parse::<usize>() {
                                if seen.insert(n) {
                                    println!("  {n}");
                                }
                            }
                        }
                    }
                    return Ok(());
                }
            },
            None => 0,
        };

        println!();
        print_separator();
        println!("  HELIX MODE TUTORIAL  (Ctrl-C to exit, --chapter N to skip ahead)");
        print_separator();
        println!();
        println!("  Prompt indicators:  NOR (normal)  INS (insert)  SEL (select)");

        while index < total {
            let lesson = &all_lessons[index];
            print_lesson(index, total, lesson);

            if !lesson.preload.is_empty() {
                preload_buffer(&mut line_editor, &helix_state, lesson.preload);
            }

            // Inner loop: keep prompting until the exercise passes.
            loop {
                match line_editor.read_line(&prompt)? {
                    Signal::Success(buf) => {
                        if (lesson.check)(&buf) {
                            break; // advance to next lesson
                        }
                        if !lesson.hint.is_empty() {
                            println!("  Not quite. {}", lesson.hint);
                            println!();
                        }
                        // Re-load the buffer for another attempt.
                        if !lesson.preload.is_empty() {
                            preload_buffer(&mut line_editor, &helix_state, lesson.preload);
                        }
                    }
                    Signal::CtrlC | Signal::CtrlD => {
                        println!("\n  Tutorial interrupted. Run again to resume.");
                        return Ok(());
                    }
                }
            }

            index += 1;
        }

        // Switch to sandbox after completing all lessons.
        println!();
        println!("  Switching to sandbox mode. Experiment freely!");
        println!("  Modes:  > (normal)   : (insert)   v (select)");
        println!("  Exit:   Ctrl-C / Ctrl-D\n");
        prompt.verbose = false;

        loop {
            match line_editor.read_line(&prompt)? {
                Signal::Success(buf) => println!("{buf}"),
                Signal::CtrlC | Signal::CtrlD => {
                    println!("\nGoodbye!");
                    break Ok(());
                }
            }
        }
    }
}
