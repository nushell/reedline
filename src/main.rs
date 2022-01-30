use reedline::{EditCommand, PromptEditMode};
use strum::IntoEnumIterator;
use {
    crossterm::{
        event::{poll, Event, KeyCode, KeyEvent, KeyModifiers},
        terminal, Result,
    },
    nu_ansi_term::{Color, Style},
    reedline::{
        default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
        CompletionMenu, DefaultCompleter, DefaultHinter, DefaultPrompt, EditMode, Emacs,
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
    if args.len() > 1 && args[1] == "--list" {
        list_stuff()?;
        println!();
        return Ok(());
    }

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
    let completion_menu = Box::new(CompletionMenu::default());
    let history_menu = Box::new(HistoryMenu::default());
    line_editor = line_editor
        .with_menu(completion_menu)
        .with_menu(history_menu);

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
        KeyCode::Char('x'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("history_menu".to_string()),
            ReedlineEvent::MenuPageNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        KeyCode::Char('x'),
        ReedlineEvent::MenuPagePrevious,
    );

    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
}

#[derive(Debug)]
struct KeyCodes;
impl KeyCodes {
    pub fn iterator() -> std::slice::Iter<'static, KeyCode> {
        static KEYCODE: [KeyCode; 29] = [
            crossterm::event::KeyCode::Backspace,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyCode::Home,
            crossterm::event::KeyCode::End,
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyCode::PageDown,
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyCode::BackTab,
            crossterm::event::KeyCode::Delete,
            crossterm::event::KeyCode::Insert,
            crossterm::event::KeyCode::F(1),
            crossterm::event::KeyCode::F(2),
            crossterm::event::KeyCode::F(3),
            crossterm::event::KeyCode::F(4),
            crossterm::event::KeyCode::F(5),
            crossterm::event::KeyCode::F(6),
            crossterm::event::KeyCode::F(7),
            crossterm::event::KeyCode::F(8),
            crossterm::event::KeyCode::F(9),
            crossterm::event::KeyCode::F(10),
            crossterm::event::KeyCode::F(11),
            crossterm::event::KeyCode::F(12),
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyCode::Null,
            crossterm::event::KeyCode::Esc,
        ];
        KEYCODE.iter()
    }
}

fn list_stuff() -> Result<()> {
    println!("--Key Modifiers--");
    for mods in get_reedline_keybinding_modifiers().iter() {
        print!("{}\n", mods);
    }

    println!("\n--Modes--");
    for modes in get_reedline_prompt_edit_modes().iter() {
        print!("{}\n", modes);
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

    Ok(())
}

/// Return a Vec of the Reedline Keybinding Modifiers
pub fn get_reedline_keybinding_modifiers() -> Vec<String> {
    let mut modifiers = vec![];
    modifiers.push("Alt".to_string());
    modifiers.push("Control".to_string());
    modifiers.push("Shift".to_string());
    modifiers.push("None".to_string());
    modifiers
}

/// Return a Vec<String> of the Reedline PromptEditModes
pub fn get_reedline_prompt_edit_modes() -> Vec<String> {
    let mut modes = vec![];
    for em in PromptEditMode::iter() {
        modes.push(em.to_string());
    }
    modes
}

/// Return a Vec<String> of the Reedline KeyCodes
pub fn get_reedline_keycodes() -> Vec<String> {
    let mut keycodes = vec![];
    for kc in KeyCodes::iterator() {
        // TODO: Perhaps this should be impl Display so we can control the output
        keycodes.push(format!("{:?}", kc));
    }
    keycodes
}

/// Return a Vec<String> of the Reedline ReedlineEvents
pub fn get_reedline_reedline_events() -> Vec<String> {
    let mut rles = vec![];
    for rle in ReedlineEvent::iter() {
        // TODO: Perhaps this should be impl Display so we can control the output
        rles.push(format!("{:?}", rle));
    }
    rles
}

/// Return a Vec<String> of the Reedline EditCommands
pub fn get_reedline_edit_commands() -> Vec<String> {
    let mut ecs = vec![];
    for edit in EditCommand::iter() {
        // TODO: Perhaps this should be impl Display so we can control the output
        ecs.push(format!("{:?}", edit));
    }
    ecs
}
