use {
    crossterm::{
        event::{poll, Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, Result,
    },
    nu_ansi_term::{Color, Style},
    reedline::{
        default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
        ContextMenu, DefaultCompleter, DefaultHinter, DefaultPrompt, EditMode, Emacs,
        ExampleHighlighter, FileBackedHistory, HistoryMenu, Keybindings, Reedline, ReedlineEvent,
        Signal, Vi,
    },
    std::{
        io::{stdout, Write},
        time::Duration,
    },
};

fn main() -> Result<()> {
    // quick command like parameter handling
    let vi_mode = matches!(std::env::args().nth(1), Some(x) if x == "--vi");
    let debug_mode = matches!(std::env::args().nth(2), Some(x) if x == "--debug");
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        println!("Ready to print events (Abort with ESC):");
        print_events()?;
        println!();
        return Ok(());
    };

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
    ];

    let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));

    let mut line_editor = Reedline::create()?
        .with_history(history)?
        .with_completer(completer)
        .with_highlighter(Box::new(ExampleHighlighter::new(commands)))
        .with_hinter(Box::new(
            DefaultHinter::default().with_style(Style::new().fg(Color::DarkGray)),
        ))
        .with_ansi_colors(true);

    // Adding default menus for the compiled reedline
    let context_menu = Box::new(ContextMenu::default());
    let history_menu = Box::new(HistoryMenu::default());
    line_editor = line_editor.with_menu(context_menu).with_menu(history_menu);

    let edit_mode: Box<dyn EditMode> = if vi_mode {
        let mut normal_keybindings = default_vi_normal_keybindings();
        let mut insert_keybindings = default_vi_insert_keybindings();

        add_menu_keybindings(&mut normal_keybindings);
        add_menu_keybindings(&mut insert_keybindings);

        Box::new(Vi::new(insert_keybindings, normal_keybindings))
    } else {
        let mut keybindings = default_emacs_keybindings();
        add_menu_keybindings(&mut keybindings);

        Box::new(Emacs::new(keybindings))
    };

    line_editor = line_editor.with_edit_mode(edit_mode);

    if debug_mode {
        line_editor = line_editor.with_debug_mode();
    }

    let prompt = DefaultPrompt::new();

    loop {
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::CtrlD) => {
                break;
            }
            Ok(Signal::Success(buffer)) => {
                if (buffer.trim() == "exit") || (buffer.trim() == "logout") {
                    break;
                }
                if buffer.trim() == "clear" {
                    line_editor.clear_screen()?;
                    continue;
                }
                if buffer.trim() == "history" {
                    line_editor.print_history()?;
                    continue;
                }
                println!("Our buffer: {}", buffer);
            }
            Ok(Signal::CtrlC) => {
                // Prompt has been cleared and should start on the next line
            }
            Ok(Signal::CtrlL) => {
                line_editor.clear_screen()?;
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
    stdout().flush()?;
    terminal::enable_raw_mode()?;
    let result = print_events_helper();
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

            if let Event::Key(KeyEvent { code, modifiers }) = event {
                match code {
                    KeyCode::Char(c) => {
                        println!(
                            "Char: {} code: {:#08x}; Modifier {:?}; Flags {:#08b}\r",
                            c,
                            u32::from(c),
                            modifiers,
                            modifiers
                        );
                    }
                    _ => {
                        println!(
                            "Keycode: {:?}; Modifier {:?}; Flags {:#08b}\r",
                            code, modifiers, modifiers
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
        KeyCode::Char('i'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("history_menu".to_string()),
            ReedlineEvent::MenuPageNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        KeyCode::Char('i'),
        ReedlineEvent::MenuPagePrevious,
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("context_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
}
