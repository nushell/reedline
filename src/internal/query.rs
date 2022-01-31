use crate::EditCommand;
use crate::PromptEditMode;
use crate::ReedlineEvent;
use crossterm::event::KeyCode;
use strum::IntoEnumIterator;

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

/// List all keybinding information
pub fn get_all_keybinding_info() {
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
}

/// Return a Vec of the Reedline Keybinding Modifiers
pub fn get_reedline_keybinding_modifiers() -> Vec<String> {
    vec![
        "Alt".to_string(),
        "Control".to_string(),
        "Shift".to_string(),
        "None".to_string(),
    ]
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
