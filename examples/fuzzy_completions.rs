// Modifies the completions example to demonstrate highlighting of fuzzy completions
// cargo run --example fuzzy_completions
//
// One of the suggestions is "multiple 汉 by̆tes字👩🏾". Try typing in "y" or "👩" and note how
// the entire grapheme "y̆" or "👩🏾" is highlighted (might not look right in your terminal).

use reedline::{
    default_emacs_keybindings, ColumnarMenu, Completer, DefaultPrompt, EditCommand, Emacs, KeyCode,
    KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, Span,
    Suggestion,
};
use std::io;

struct HomegrownFuzzyCompleter(Vec<String>);

impl Completer for HomegrownFuzzyCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        // Grandma's fuzzy matching recipe. She swears it's better than that crates.io-bought stuff
        self.0
            .iter()
            .filter_map(|command_str| {
                let command = command_str.as_bytes();
                let mut start = 0;
                let mut match_indices = Vec::new();
                for l in line.as_bytes() {
                    if start == command.len() {
                        break;
                    }
                    let mut i = start;
                    while i < command.len() && *l != command[i] {
                        i += 1;
                    }
                    if i < command.len() {
                        match_indices.push(i);
                        start = i + 1;
                    }
                }
                if match_indices.is_empty() || match_indices.len() * 2 < pos {
                    None
                } else {
                    Some(Suggestion {
                        value: command_str.to_string(),
                        description: None,
                        style: None,
                        extra: None,
                        span: Span::new(pos - line.len(), pos),
                        append_whitespace: false,
                        match_indices: Some(match_indices),
                    })
                }
            })
            .collect()
    }
}

fn add_menu_keybindings(keybindings: &mut Keybindings) {
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    keybindings.add_binding(
        KeyModifiers::ALT,
        KeyCode::Enter,
        ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
    );
}

fn main() -> io::Result<()> {
    // Number of columns
    let columns: u16 = 4;
    // Column width
    let col_width: Option<usize> = None;
    // Column padding
    let col_padding: usize = 2;

    let commands = vec![
        "test".into(),
        "clear".into(),
        "exit".into(),
        "history 1".into(),
        "history 2".into(),
        "logout".into(),
        "login".into(),
        "hello world".into(),
        "hello world reedline".into(),
        "hello world something".into(),
        "hello world another".into(),
        "hello world 1".into(),
        "hello world 2".into(),
        "hello another very large option for hello word that will force one column".into(),
        "this is the reedline crate".into(),
        "abaaabas".into(),
        "abaaacas".into(),
        "ababac".into(),
        "abacaxyc".into(),
        "abadarabc".into(),
        "multiple 汉 by̆tes字👩🏾".into(),
    ];

    let completer = Box::new(HomegrownFuzzyCompleter(commands));

    // Use the interactive menu to select options from the completer
    let columnar_menu = ColumnarMenu::default()
        .with_name("completion_menu")
        .with_columns(columns)
        .with_column_width(col_width)
        .with_column_padding(col_padding);

    let completion_menu = Box::new(columnar_menu);

    let mut keybindings = default_emacs_keybindings();
    add_menu_keybindings(&mut keybindings);

    let edit_mode = Box::new(Emacs::new(keybindings));

    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode);

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
        }
    }
}
