use crate::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    EditCommand, Keybindings, PromptEditMode, ReedlineEvent,
};
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

/// Return a `Vec` of the Reedline Keybinding Modifiers
pub fn get_reedline_keybinding_modifiers() -> Vec<String> {
    vec![
        "Alt".to_string(),
        "Control".to_string(),
        "Shift".to_string(),
        "None".to_string(),
    ]
}

/// Return a `Vec<String>` of the Reedline [`PromptEditMode`]s
pub fn get_reedline_prompt_edit_modes() -> Vec<String> {
    PromptEditMode::iter().map(|em| em.to_string()).collect()
}

/// Return a `Vec<String>` of the Reedline `KeyCode`s
pub fn get_reedline_keycodes() -> Vec<String> {
    KeyCodes::iterator().map(|kc| format!("{:?}", kc)).collect()
}

/// Return a `Vec<String>` of the Reedline [`ReedlineEvent`]s
pub fn get_reedline_reedline_events() -> Vec<String> {
    ReedlineEvent::iter()
        .map(|rle| format!("{:?}", rle))
        .collect()
}

/// Return a `Vec<String>` of the Reedline [`EditCommand`]s
pub fn get_reedline_edit_commands() -> Vec<String> {
    EditCommand::iter()
        .map(|edit| format!("{:?}", edit))
        .collect()
}

/// Get the default keybindings and return a `Vec<(String, String, String, String)>`
/// where String 1 is `mode`, String 2 is `key_modifiers`, String 3 is `key_code`, and
/// Sting 4 is `event`
pub fn get_reedline_default_keybindings() -> Vec<(String, String, String, String)> {
    let options = vec![
        ("emacs", default_emacs_keybindings()),
        ("vi_normal", default_vi_normal_keybindings()),
        ("vi_insert", default_vi_insert_keybindings()),
    ];

    options
        .into_iter()
        .flat_map(|(mode, keybindings)| get_keybinding_strings(mode, &keybindings))
        .collect()
}

fn get_keybinding_strings(
    mode: &str,
    keybindings: &Keybindings,
) -> Vec<(String, String, String, String)> {
    keybindings
        .get_keybindings()
        .iter()
        .map(|(combination, event)| {
            (
                mode.to_string(),
                format!("{:?}", combination.modifier),
                format!("{:?}", combination.key_code),
                format!("{:?}", event),
            )
        })
        .collect()
}
