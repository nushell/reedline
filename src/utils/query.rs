use crate::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    EditCommand, Keybindings, PromptEditMode, ReedlineEvent,
};
use crossterm::event::KeyCode;
use std::fmt::{Display, Formatter};
use strum::IntoEnumIterator;

struct ReedLineCrossTermKeyCode(crossterm::event::KeyCode);
impl ReedLineCrossTermKeyCode {
    fn iterator() -> std::slice::Iter<'static, ReedLineCrossTermKeyCode> {
        static KEYCODE: [ReedLineCrossTermKeyCode; 19] = [
            ReedLineCrossTermKeyCode(KeyCode::Backspace),
            ReedLineCrossTermKeyCode(KeyCode::Enter),
            ReedLineCrossTermKeyCode(KeyCode::Left),
            ReedLineCrossTermKeyCode(KeyCode::Right),
            ReedLineCrossTermKeyCode(KeyCode::Up),
            ReedLineCrossTermKeyCode(KeyCode::Down),
            ReedLineCrossTermKeyCode(KeyCode::Home),
            ReedLineCrossTermKeyCode(KeyCode::End),
            ReedLineCrossTermKeyCode(KeyCode::PageUp),
            ReedLineCrossTermKeyCode(KeyCode::PageDown),
            ReedLineCrossTermKeyCode(KeyCode::Tab),
            ReedLineCrossTermKeyCode(KeyCode::BackTab),
            ReedLineCrossTermKeyCode(KeyCode::Delete),
            ReedLineCrossTermKeyCode(KeyCode::Insert),
            ReedLineCrossTermKeyCode(KeyCode::F(1)),
            ReedLineCrossTermKeyCode(KeyCode::Char(' ')),
            ReedLineCrossTermKeyCode(KeyCode::Char('a')),
            ReedLineCrossTermKeyCode(KeyCode::Null),
            ReedLineCrossTermKeyCode(KeyCode::Esc),
        ];
        KEYCODE.iter()
    }
}

impl Display for ReedLineCrossTermKeyCode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ReedLineCrossTermKeyCode(kc) => match kc {
                KeyCode::Backspace => write!(f, "Backspace"),
                KeyCode::Enter => write!(f, "Enter"),
                KeyCode::Left => write!(f, "Left"),
                KeyCode::Right => write!(f, "Right"),
                KeyCode::Up => write!(f, "Up"),
                KeyCode::Down => write!(f, "Down"),
                KeyCode::Home => write!(f, "Home"),
                KeyCode::End => write!(f, "End"),
                KeyCode::PageUp => write!(f, "PageUp"),
                KeyCode::PageDown => write!(f, "PageDown"),
                KeyCode::Tab => write!(f, "Tab"),
                KeyCode::BackTab => write!(f, "BackTab"),
                KeyCode::Delete => write!(f, "Delete"),
                KeyCode::Insert => write!(f, "Insert"),
                KeyCode::F(_) => write!(f, "F<number>"),
                KeyCode::Char(' ') => write!(f, "Space"),
                KeyCode::Char(_) => write!(f, "Char_<letter>"),
                KeyCode::Null => write!(f, "Null"),
                KeyCode::Esc => write!(f, "Esc"),
                KeyCode::CapsLock => write!(f, "CapsLock"),
                KeyCode::ScrollLock => write!(f, "ScrollLock"),
                KeyCode::NumLock => write!(f, "NumLock"),
                KeyCode::PrintScreen => write!(f, "PrintScreen"),
                KeyCode::Pause => write!(f, "Pause"),
                KeyCode::Menu => write!(f, "Menu"),
                KeyCode::KeypadBegin => write!(f, "KeypadBegin"),
                KeyCode::Media(_) => write!(f, "Media<media>"),
                KeyCode::Modifier(_) => write!(f, "Modifier<modifier>"),
            },
        }
    }
}

/// Return a `Vec` of the Reedline Keybinding Modifiers
pub fn get_reedline_keybinding_modifiers() -> Vec<String> {
    vec![
        "Alt".to_string(),
        "Control".to_string(),
        "Shift".to_string(),
        "None".to_string(),
        "Shift_Alt".to_string(),
        "Alt_Shift".to_string(),
        "Control_Shift".to_string(),
        "Shift_Control".to_string(),
        "Control_Alt".to_string(),
        "Alt_Control".to_string(),
        "Control_Alt_Shift".to_string(),
        "Control_Shift_Alt".to_string(),
    ]
}

/// Return a `Vec<String>` of the Reedline [`PromptEditMode`]s
pub fn get_reedline_prompt_edit_modes() -> Vec<String> {
    PromptEditMode::iter().map(|em| em.to_string()).collect()
}

/// Return a `Vec<String>` of the Reedline `KeyCode`s
pub fn get_reedline_keycodes() -> Vec<String> {
    ReedLineCrossTermKeyCode::iterator()
        .map(|kc| format!("{kc}"))
        .collect()
}

/// Return a `Vec<String>` of the Reedline [`ReedlineEvent`]s
pub fn get_reedline_reedline_events() -> Vec<String> {
    ReedlineEvent::iter().map(|rle| rle.to_string()).collect()
}

/// Return a `Vec<String>` of the Reedline [`EditCommand`]s
pub fn get_reedline_edit_commands() -> Vec<String> {
    EditCommand::iter().map(|edit| edit.to_string()).collect()
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
    let mut data: Vec<(String, String, String, String)> = keybindings
        .get_keybindings()
        .iter()
        .map(|(combination, event)| {
            (
                mode.to_string(),
                format!("{:?}", combination.modifier),
                format!("{:?}", combination.key_code),
                format!("{event:?}"),
            )
        })
        .collect();

    data.sort();

    data
}
