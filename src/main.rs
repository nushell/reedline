#[cfg(not(feature = "sqlite"))]
use reedline::FileBackedHistory;

use {
    crossterm::{
        event::{
            poll, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture,
            EnableBracketedPaste, EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEvent,
            KeyEventKind, KeyEventState, KeyModifiers, KeyboardEnhancementFlags,
            PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
        },
        execute, terminal, Result,
    },
    nu_ansi_term::{Color, Style},
    reedline::{
        default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
        get_reedline_default_keybindings, get_reedline_edit_commands,
        get_reedline_keybinding_modifiers, get_reedline_keycodes, get_reedline_prompt_edit_modes,
        get_reedline_reedline_events, ColumnarMenu, DefaultCompleter, DefaultHinter, DefaultPrompt,
        DefaultValidator, EditCommand, EditMode, Emacs, ExampleHighlighter, Keybindings, ListMenu,
        Reedline, ReedlineEvent, ReedlineMenu, Signal, Vi,
    },
    std::{
        io::{stdout, Write},
        time::Duration,
    },
};

fn main() -> Result<()> {
    // quick command like parameter handling
    let vi_mode = matches!(std::env::args().nth(1), Some(x) if x == "--vi");
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        println!("Ready to print events (Abort with ESC):");
        print_events()?;
        println!();
        return Ok(());
    };
    if args.len() > 1 && args[1] == "--list" {
        get_all_keybinding_info();
        println!();
        return Ok(());
    }

    #[cfg(feature = "sqlite")]
    let history = Box::new(
        reedline::SqliteBackedHistory::with_file("history.sqlite3".into())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
    );
    #[cfg(not(feature = "sqlite"))]
    let history = Box::new(FileBackedHistory::with_file(50, "history.txt".into())?);
    let commands = vec![
        "test".into(),
        "clear".into(),
        "exit".into(),
        "history 1".into(),
        "history 2".into(),
        "history 3".into(),
        "history 4".into(),
        "history 5".into(),
        "logout".into(),
        "login".into(),
        "hello world".into(),
        "hello world reedline".into(),
        "hello world something".into(),
        "hello world another".into(),
        "hello world 1".into(),
        "hello world 2".into(),
        "hello world 3".into(),
        "hello world 4".into(),
        "hello another very large option for hello word that will force one column".into(),
        "this is the reedline crate".into(),
        "abaaacas".into(),
        "abaaac".into(),
        "abaaaxyc".into(),
        "abaaarabc".into(),
        "こんにちは世界".into(),
        "こんばんは世界".into(),
    ];

    let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));

    let mut line_editor = Reedline::create()
        .with_history(history)
        .with_completer(completer)
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_highlighter(Box::new(ExampleHighlighter::new(commands)))
        .with_hinter(Box::new(
            DefaultHinter::default().with_style(Style::new().fg(Color::DarkGray)),
        ))
        .with_validator(Box::new(DefaultValidator))
        .with_ansi_colors(true);

    // Adding default menus for the compiled reedline
    line_editor = line_editor
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(
            ColumnarMenu::default().with_name("completion_menu"),
        )))
        .with_menu(ReedlineMenu::HistoryMenu(Box::new(
            ListMenu::default().with_name("history_menu"),
        )));

    let edit_mode: Box<dyn EditMode> = if vi_mode {
        let mut normal_keybindings = default_vi_normal_keybindings();
        let mut insert_keybindings = default_vi_insert_keybindings();

        add_menu_keybindings(&mut normal_keybindings);
        add_menu_keybindings(&mut insert_keybindings);

        add_newline_keybinding(&mut insert_keybindings);

        Box::new(Vi::new(insert_keybindings, normal_keybindings))
    } else {
        let mut keybindings = default_emacs_keybindings();
        add_menu_keybindings(&mut keybindings);
        add_newline_keybinding(&mut keybindings);

        Box::new(Emacs::new(keybindings))
    };

    line_editor = line_editor.with_edit_mode(edit_mode);

    // Adding vi as text editor
    line_editor = line_editor.with_buffer_editor("vi".into(), "nu".into());

    let prompt = DefaultPrompt::new();

    loop {
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::CtrlD) => {
                break;
            }
            Ok(Signal::Success(buffer)) => {
                #[cfg(feature = "sqlite")]
                let start = std::time::Instant::now();
                // save timestamp, cwd, hostname to history
                #[cfg(feature = "sqlite")]
                if !buffer.is_empty() {
                    line_editor
                        .update_last_command_context(&|mut c: reedline::HistoryItem| {
                            c.start_timestamp = Some(chrono::Utc::now());
                            c.hostname =
                                Some(gethostname::gethostname().to_string_lossy().to_string());
                            c.cwd = std::env::current_dir()
                                .ok()
                                .map(|e| e.to_string_lossy().to_string());
                            c
                        })
                        .expect("todo: error handling");
                }
                if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                    break;
                }
                if buffer.trim() == "clear" {
                    line_editor.clear_scrollback()?;
                    continue;
                }
                if buffer.trim() == "history" {
                    line_editor.print_history()?;
                    continue;
                }
                println!("Our buffer: {}", buffer);
                #[cfg(feature = "sqlite")]
                if !buffer.is_empty() {
                    line_editor
                        .update_last_command_context(&|mut c| {
                            c.duration = Some(start.elapsed());
                            c.exit_status = Some(0);
                            c
                        })
                        .expect("todo: error handling");
                }
            }
            Ok(Signal::CtrlC) => {
                // Prompt has been cleared and should start on the next line
            }
            Err(err) => {
                println!("Error: {:?}", err);
            }
        }
    }

    println!();
    Ok(())
}

/// **For debugging purposes only:** Track the terminal events observed by [`Reedline`] and print them.
pub fn print_events() -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    // stdout().flush()?;
    execute!(
        stdout,
        EnableBracketedPaste,
        EnableFocusChange,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;

    let result = print_events_helper();

    execute!(
        stdout,
        DisableBracketedPaste,
        PopKeyboardEnhancementFlags,
        DisableFocusChange,
        DisableMouseCapture
    )?;

    terminal::disable_raw_mode()?;

    result
}

// this fn is totally ripped off from crossterm's examples
// it's really a diagnostic routine to see if crossterm is
// even seeing the events. if you press a key and no events
// are printed, it's a good chance your terminal is eating
// those events.
fn print_events_helper() -> Result<()> {
    loop {
        // Wait up to 5s for another event
        if poll(Duration::from_millis(5_000))? {
            // It's guaranteed that read() wont block if `poll` returns `Ok(true)`
            let event = crossterm::event::read()?;

            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                state,
            }) = event
            {
                match code {
                    KeyCode::Char(c) => {
                        println!(
                            "Char: {} code: {:#08x}; Modifier {:?}; Flags {:#08b} Kind: {:?} State: {:?}\r",
                            c,
                            u32::from(c),
                            modifiers,
                            modifiers,
                            kind,
                            state,
                        );
                    }
                    _ => {
                        println!(
                            "Keycode: {:?}; Modifier {:?}; Flags {:#08b} Kind: {:?} State: {:?}\r",
                            code, modifiers, modifiers, kind, state,
                        );
                    }
                }
            } else {
                println!("Event::{:?}\r", event);
            }

            // hit the esc key to git out
            if event == Event::Key(KeyCode::Esc.into()) {
                break;
            }
        } else {
            // Timeout expired, no event for 5s
            println!("Waiting for you to type...\r");
        }
    }

    Ok(())
}

fn add_menu_keybindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('x'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("history_menu".to_string()),
            ReedlineEvent::MenuPageNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        KeyCode::Char('x'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::MenuPagePrevious,
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::MenuPrevious,
    );
}

fn add_newline_keybinding(keybindings: &mut Keybindings) {
    // This doesn't work for macOS
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
    );
}

/// List all keybinding information
fn get_all_keybinding_info() {
    println!("--Key Modifiers--");
    for mods in get_reedline_keybinding_modifiers().iter() {
        println!("{}", mods);
    }

    println!("\n--Modes--");
    for modes in get_reedline_prompt_edit_modes().iter() {
        println!("{}", modes);
    }

    println!("\n--Key Codes--");
    for kcs in get_reedline_keycodes().iter() {
        println!("{}", kcs);
    }

    println!("\n--Reedline Events--");
    for rle in get_reedline_reedline_events().iter() {
        println!("{}", rle);
    }

    println!("\n--Edit Commands--");
    for edit in get_reedline_edit_commands().iter() {
        println!("{}", edit);
    }

    println!("\n--Default Keybindings--");
    for (mode, modifier, code, kind, state, event) in get_reedline_default_keybindings() {
        println!(
            "mode: {}, keymodifiers: {}, keycode: {}, kind: {}, state: {}, event: {}",
            mode, modifier, code, kind, state, event
        );
    }
}
