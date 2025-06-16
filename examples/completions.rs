// Create a reedline object with tab completions support
// cargo run --example completions
//
// "t" [Tab] will allow you to select the completions "test" and "this is the reedline crate"
// [Enter] to select the chosen alternative

use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultPrompt, EditCommand, Emacs,
    KeyCode, KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal,
};
use std::io;

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
    ];

    let completer = Box::new(DefaultCompleter::new_with_wordlen(commands, 2));

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
