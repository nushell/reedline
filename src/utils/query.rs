use crate::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    Keybindings,
};
use crossterm::event::KeyCode;
use std::fmt::{Display, Formatter};

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

/// Return a `Vec<String>` of the Reedline `KeyCode`s
pub fn get_reedline_keycodes() -> Vec<String> {
    ReedLineCrossTermKeyCode::iterator()
        .map(|kc| format!("{kc}"))
        .collect()
}

/// Get the default keybindings and return a `Vec<(String, String, String, String)>`
/// where String 1 is `mode`, String 2 is `key_modifiers`, String 3 is `key_code`, and
/// Sting 4 is `event`
pub fn get_reedline_default_keybindings() -> Vec<(String, String, String, String)> {
    #[allow(unused_mut)]
    let mut options = vec![
        ("emacs", default_emacs_keybindings()),
        ("vi_normal", default_vi_normal_keybindings()),
        ("vi_insert", default_vi_insert_keybindings()),
    ];

    // Only the table layer; the bulk of the helix bindings live in its state
    // machine, which `Keybindings` iteration cannot see.
    options.extend([
        ("helix_normal", crate::default_helix_normal_keybindings()),
        ("helix_insert", crate::default_helix_insert_keybindings()),
    ]);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keycodes_are_listed_by_name() {
        assert_eq!(
            get_reedline_keycodes(),
            [
                "Backspace",
                "Enter",
                "Left",
                "Right",
                "Up",
                "Down",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Tab",
                "BackTab",
                "Delete",
                "Insert",
                "F<number>",
                "Space",
                "Char_<letter>",
                "Null",
                "Esc",
            ]
        );
    }

    #[test]
    fn modifiers_are_listed_by_name() {
        assert_eq!(
            get_reedline_keybinding_modifiers(),
            [
                "Alt",
                "Control",
                "Shift",
                "None",
                "Shift_Alt",
                "Alt_Shift",
                "Control_Shift",
                "Shift_Control",
                "Control_Alt",
                "Alt_Control",
                "Control_Alt_Shift",
                "Control_Shift_Alt",
            ]
        );
    }

    #[test]
    fn default_keybindings_cover_every_mode() {
        let bindings = get_reedline_default_keybindings();
        let modes: HashSet<_> = bindings.iter().map(|(mode, ..)| mode.as_str()).collect();
        assert_eq!(
            modes,
            HashSet::from([
                "emacs",
                "vi_normal",
                "vi_insert",
                "helix_normal",
                "helix_insert"
            ])
        );
    }

    #[test]
    fn default_keybindings_are_sorted_within_each_mode() {
        let bindings = get_reedline_default_keybindings();
        for mode in [
            "emacs",
            "vi_normal",
            "vi_insert",
            "helix_normal",
            "helix_insert",
        ] {
            let rows: Vec<_> = bindings.iter().filter(|(m, ..)| m == mode).collect();
            assert!(!rows.is_empty(), "no bindings for {mode}");
            assert!(rows.is_sorted(), "{mode} bindings are not sorted");
        }
    }

    #[test]
    fn emacs_ctrl_c_is_listed_as_strings() {
        let bindings = get_reedline_default_keybindings();
        assert!(bindings
            .iter()
            .any(|(mode, modifier, key, event)| mode == "emacs"
                && modifier == "KeyModifiers(CONTROL)"
                && key == "Char('c')"
                && event == "CtrlC"));
    }
}
