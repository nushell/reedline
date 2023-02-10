use {
    crossterm::{
        event::{KeyCode, KeyModifiers},
        Result,
    },
    nu_ansi_term::{Color, Style},
    reedline::{
        default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
        ColumnarMenu, DefaultCompleter, DefaultHinter, DefaultPrompt, DefaultValidator,
        EditCommand, EditMode, Emacs, ExampleHighlighter, Keybindings, ListMenu, Reedline,
        ReedlineEvent, ReedlineMenu, Signal, Vi,
    },
};

use crossterm::cursor::CursorShape;
use reedline::CursorConfig;
#[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
use reedline::FileBackedHistory;

fn main() -> Result<()> {
    println!("Ctrl-D to quit");
    // quick command like parameter handling
    let vi_mode = matches!(std::env::args().nth(1), Some(x) if x == "--vi");

    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    let history = Box::new(
        reedline::SqliteBackedHistory::with_file("history.sqlite3".into())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
    );
    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
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

    let cursor_config = CursorConfig {
        vi_insert: Some(CursorShape::Line),
        vi_normal: Some(CursorShape::Block),
        emacs: None,
    };

    let mut line_editor = Reedline::create()
        .with_history(history)
        .with_completer(completer)
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_cursor_config(cursor_config)
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

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::CtrlD) => {
                break;
            }
            Ok(Signal::Success(buffer)) => {
                #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
                let start = std::time::Instant::now();
                // save timestamp, cwd, hostname to history
                #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
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
                if buffer.trim() == "clear-history" {
                    let hstry = Box::new(line_editor.history_mut());
                    hstry
                        .clear()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    continue;
                }
                println!("Our buffer: {buffer}");
                #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
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
                println!("Error: {err:?}");
            }
        }
    }

    println!();
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
            ReedlineEvent::Edit(vec![EditCommand::Complete]),
        ]),
    );

    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::MenuPrevious,
    );
}

fn add_newline_keybinding(keybindings: &mut Keybindings) {
    // This doesn't work for macOS
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Enter,
        ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
    );
}
