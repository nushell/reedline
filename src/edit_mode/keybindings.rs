use std::collections::HashMap;

use crate::enums::ReedlineEvent;

use {
    crate::EditCommand,
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombination {
    modifier: KeyModifiers,
    key_code: KeyCode,
}

/// Main definition of editor keybindings
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Keybindings {
    /// Defines a keybinding for a reedline event
    pub bindings: HashMap<KeyCombination, ReedlineEvent>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self::new()
    }
}

impl Keybindings {
    /// New keybining
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Defines an empty keybinding object
    pub fn empty() -> Self {
        Self::new()
    }

    /// Adds a keybinding
    pub fn add_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        commands: Vec<ReedlineEvent>,
    ) {
        let command = if commands.len() == 1 {
            commands
                .into_iter()
                .next()
                .expect("already checked that has one element")
        } else {
            ReedlineEvent::UntilFound(commands)
        };

        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.insert(key_combo, command);
    }

    /// Find a keybinding based on the modifier and keycode
    pub fn find_binding(&self, modifier: KeyModifiers, key_code: KeyCode) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.get(&key_combo).cloned()
    }
}

pub fn edit_bind(command: EditCommand) -> ReedlineEvent {
    ReedlineEvent::Edit(vec![command])
}

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = Keybindings::new();

    // CTRL
    kb.add_binding(KM::CONTROL, KC::Left, vec![edit_bind(EC::MoveWordLeft)]);
    kb.add_binding(KM::CONTROL, KC::Right, vec![edit_bind(EC::MoveWordRight)]);
    kb.add_binding(KM::CONTROL, KC::Delete, vec![edit_bind(EC::DeleteWord)]);
    kb.add_binding(
        KM::CONTROL,
        KC::Backspace,
        vec![edit_bind(EC::BackspaceWord)],
    );
    kb.add_binding(KM::CONTROL, KC::End, vec![edit_bind(EC::MoveToEnd)]);
    kb.add_binding(KM::CONTROL, KC::Home, vec![edit_bind(EC::MoveToStart)]);
    kb.add_binding(KM::CONTROL, KC::Char('d'), vec![ReedlineEvent::CtrlD]);
    kb.add_binding(KM::CONTROL, KC::Char('c'), vec![ReedlineEvent::CtrlC]);
    kb.add_binding(KM::CONTROL, KC::Char('g'), vec![edit_bind(EC::Redo)]);
    kb.add_binding(KM::CONTROL, KC::Char('z'), vec![edit_bind(EC::Undo)]);
    kb.add_binding(
        KM::CONTROL,
        KC::Char('a'),
        vec![edit_bind(EC::MoveToLineStart)],
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('e'),
        vec![edit_bind(EC::MoveToLineEnd)],
    );
    kb.add_binding(KM::CONTROL, KC::Char('k'), vec![edit_bind(EC::CutToEnd)]);
    kb.add_binding(
        KM::CONTROL,
        KC::Char('u'),
        vec![edit_bind(EC::CutFromStart)],
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('y'),
        vec![edit_bind(EC::PasteCutBufferBefore)],
    );
    kb.add_binding(KM::CONTROL, KC::Char('b'), vec![ReedlineEvent::Left]);
    kb.add_binding(KM::CONTROL, KC::Char('f'), vec![ReedlineEvent::Right]);
    kb.add_binding(KM::CONTROL, KC::Char('h'), vec![edit_bind(EC::Backspace)]);
    kb.add_binding(KM::CONTROL, KC::Char('w'), vec![edit_bind(EC::CutWordLeft)]);
    kb.add_binding(
        KM::CONTROL,
        KC::Char('p'),
        vec![ReedlineEvent::PreviousHistory],
    );
    kb.add_binding(KM::CONTROL, KC::Char('n'), vec![ReedlineEvent::NextHistory]);
    kb.add_binding(
        KM::CONTROL,
        KC::Char('r'),
        vec![ReedlineEvent::SearchHistory],
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('t'),
        vec![edit_bind(EC::SwapGraphemes)],
    );
    kb.add_binding(KM::CONTROL, KC::Char('l'), vec![ReedlineEvent::ClearScreen]);
    kb.add_binding(KM::ALT, KC::Char('b'), vec![edit_bind(EC::MoveWordLeft)]);
    kb.add_binding(KM::ALT, KC::Char('f'), vec![edit_bind(EC::MoveWordRight)]);
    kb.add_binding(KM::ALT, KC::Char('d'), vec![edit_bind(EC::CutWordRight)]);
    kb.add_binding(KM::ALT, KC::Char('u'), vec![edit_bind(EC::UppercaseWord)]);
    kb.add_binding(KM::ALT, KC::Char('l'), vec![edit_bind(EC::LowercaseWord)]);
    kb.add_binding(KM::ALT, KC::Char('c'), vec![edit_bind(EC::CapitalizeChar)]);
    kb.add_binding(KM::ALT, KC::Left, vec![edit_bind(EC::MoveWordLeft)]);
    kb.add_binding(KM::ALT, KC::Right, vec![edit_bind(EC::MoveWordRight)]);
    kb.add_binding(KM::ALT, KC::Delete, vec![edit_bind(EC::DeleteWord)]);
    kb.add_binding(KM::ALT, KC::Backspace, vec![edit_bind(EC::BackspaceWord)]);

    kb.add_binding(KM::NONE, KC::End, vec![edit_bind(EC::MoveToLineEnd)]);
    kb.add_binding(KM::NONE, KC::Home, vec![edit_bind(EC::MoveToLineStart)]);

    kb.add_binding(
        KM::NONE,
        KC::Up,
        vec![ReedlineEvent::Up, ReedlineEvent::MenuUp],
    );
    kb.add_binding(
        KM::NONE,
        KC::Down,
        vec![ReedlineEvent::Down, ReedlineEvent::MenuDown],
    );
    kb.add_binding(
        KM::NONE,
        KC::Left,
        vec![ReedlineEvent::Left, ReedlineEvent::MenuLeft],
    );
    kb.add_binding(
        KM::NONE,
        KC::Right,
        vec![
            ReedlineEvent::Right,
            ReedlineEvent::MenuRight,
            ReedlineEvent::Complete,
        ],
    );

    kb.add_binding(KM::NONE, KC::Delete, vec![edit_bind(EC::Delete)]);
    kb.add_binding(KM::NONE, KC::Backspace, vec![edit_bind(EC::Backspace)]);

    kb.add_binding(
        KM::NONE,
        KC::Tab,
        vec![ReedlineEvent::ContextMenu, ReedlineEvent::MenuNext],
    );
    kb.add_binding(KM::SHIFT, KC::BackTab, vec![ReedlineEvent::MenuPrevious]);
    kb.add_binding(KM::NONE, KC::Esc, vec![ReedlineEvent::Esc]);

    kb
}
