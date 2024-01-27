// Create a reedline object with tab completions support
// cargo run --example completions
//
// "t" [Tab] will allow you to select the completions "test" and "this is the reedline crate"
// [Enter] to select the chosen alternative

use reedline::{
    default_emacs_keybindings, DefaultCompleter, DefaultPrompt, DescriptionMode, EditCommand,
    Emacs, IdeMenu, KeyCode, KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent,
    ReedlineMenu, Signal,
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
    // Min width of the completion box, including the border
    let min_completion_width: u16 = 0;
    // Max width of the completion box, including the border
    let max_completion_width: u16 = 50;
    // Max height of the completion box, including the border
    let max_completion_height = u16::MAX;
    // Padding inside of the completion box (on the left and right side)
    let padding: u16 = 0;
    // Whether to draw the default border around the completion box
    let border: bool = false;
    // Offset of the cursor from the top left corner of the completion box
    // By default the top left corner is below the cursor
    let cursor_offset: i16 = 0;
    // How the description should be aligned
    let description_mode: DescriptionMode = DescriptionMode::PreferRight;
    // Min width of the description box, including the border
    let min_description_width: u16 = 0;
    // Max width of the description box, including the border
    let max_description_width: u16 = 50;
    // Distance between the completion and the description box
    let description_offset: u16 = 1;
    // If true, the cursor pos will be corrected, so the suggestions match up with the typed text
    // ```text
    // C:\> str
    //      str join
    //      str trim
    //      str split
    // ```
    // If a border is being used
    let correct_cursor_pos: bool = false;

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
    let mut ide_menu = IdeMenu::default()
        .with_name("completion_menu")
        .with_min_completion_width(min_completion_width)
        .with_max_completion_width(max_completion_width)
        .with_max_completion_height(max_completion_height)
        .with_padding(padding)
        .with_cursor_offset(cursor_offset)
        .with_description_mode(description_mode)
        .with_min_description_width(min_description_width)
        .with_max_description_width(max_description_width)
        .with_description_offset(description_offset)
        .with_correct_cursor_pos(correct_cursor_pos);

    if border {
        ide_menu = ide_menu.with_default_border();
    }

    let completion_menu = Box::new(ide_menu);

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
