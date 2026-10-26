// Create a reedline object with a status line under the buffer.
// cargo run --example status_line
//
// The status line comments on the line while you type it. This one knows
// three commands: `add <a> <b>`, `greet <name>` and `upper <text>`.
//
// "add 1 " shows `add <a> <b>` with <b>, the argument being typed, underlined.
// "add 1 2" adds `= 3`, the value the line would produce.
// "sub " shows `error: unknown command: sub` in red, once the name is typed.
// "greet w" [Tab] opens the completion menu below the status line.
// [Enter] erases the status line before the line is submitted.

use nu_ansi_term::{Color, Style};
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultPrompt, Emacs, KeyCode,
    KeyModifiers, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, StatusLine,
};
use std::io;
use unicode_width::UnicodeWidthChar;

/// The commands this example knows, with the names of their arguments.
const COMMANDS: &[(&str, &[&str])] = &[
    ("add", &["a", "b"]),
    ("greet", &["name"]),
    ("upper", &["text"]),
];

/// For custom status lines, implement the StatusLine trait. Reedline asks for
/// the status on every repaint, with the buffer, the cursor and the terminal
/// width, and draws what it gets on its own row below the buffer.
struct CommandHelp;

impl StatusLine for CommandHelp {
    fn status(&mut self, line: &str, pos: usize, columns: u16, use_ansi_coloring: bool) -> String {
        render(&describe(line, pos), columns, use_ansi_coloring)
    }
}

/// The status for `line` with the cursor at byte `pos`, as styled pieces:
/// the error in the line, or the signature of its command with the argument
/// at the cursor emphasized, then the value of a finished call.
fn describe(line: &str, pos: usize) -> Vec<(Style, String)> {
    let mut words = line.split_whitespace();
    let Some(name) = words.next() else {
        // An empty status draws no row at all.
        return vec![];
    };
    let args: Vec<&str> = words.collect();

    // The word the cursor is in, or the next one after a space; 0 is the
    // command name.
    let before = &line[..pos];
    let typed = before.split_whitespace().count();
    let current = if before.ends_with(char::is_whitespace) {
        typed
    } else {
        typed.saturating_sub(1)
    };

    let Some((_, params)) = COMMANDS.iter().find(|(command, _)| *command == name) else {
        // A name still being typed is not wrong yet.
        return if current == 0 {
            vec![]
        } else {
            vec![error(format!("unknown command: {name}"))]
        };
    };
    if args.len() > params.len() {
        return vec![error(format!(
            "{name} takes {} arguments, found {}",
            params.len(),
            args.len()
        ))];
    }

    let text = Style::new().fg(Color::DarkGray);
    let mut segments = vec![(text, name.to_string())];
    for (index, param) in params.iter().enumerate() {
        let style = if index + 1 == current {
            text.bold().underline()
        } else {
            text
        };
        segments.push((text, " ".to_string()));
        segments.push((style, format!("<{param}>")));
    }
    if let Some(value) = evaluate(name, &args) {
        segments.push((text, format!("   = {value}")));
    }
    segments
}

fn error(message: String) -> (Style, String) {
    (Style::new().fg(Color::Red), format!("error: {message}"))
}

/// The value of a call, once it has all its arguments and they make sense.
fn evaluate(name: &str, args: &[&str]) -> Option<String> {
    match (name, args) {
        ("add", [a, b]) => {
            let sum = a.parse::<i64>().ok()?.checked_add(b.parse().ok()?)?;
            Some(sum.to_string())
        }
        ("greet", [name]) => Some(format!("Hello, {name}!")),
        ("upper", [text]) => Some(text.to_uppercase()),
        _ => None,
    }
}

/// Joins `segments` into the status text, styled only with
/// `use_ansi_coloring`. Text wider than `columns` would wrap onto more rows,
/// so it is cut to one.
fn render(segments: &[(Style, String)], columns: u16, use_ansi_coloring: bool) -> String {
    // One column short: text filling the row exactly leaves the terminal's
    // wrap pending.
    let mut room = usize::from(columns).saturating_sub(1);
    let mut out = String::new();
    for (style, text) in segments {
        let mut piece = String::new();
        let mut full = false;
        for c in text.chars() {
            // Control characters have no width, and would move the cursor.
            let Some(width) = c.width() else {
                continue;
            };
            if width > room {
                full = true;
                break;
            }
            room -= width;
            piece.push(c);
        }
        if use_ansi_coloring {
            out.push_str(&style.paint(piece).to_string());
        } else {
            out.push_str(&piece);
        }
        if full {
            break;
        }
    }
    out
}

fn main() -> io::Result<()> {
    let mut words: Vec<String> = COMMANDS.iter().map(|(name, _)| name.to_string()).collect();
    words.extend(["world".into(), "wonderland".into()]);
    let completer = Box::new(DefaultCompleter::new_with_wordlen(words, 2));
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let mut line_editor = Reedline::create()
        .with_status_line(Box::new(CommandHelp))
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(Emacs::new(keybindings)));
    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
            _ => {}
        }
    }
}
