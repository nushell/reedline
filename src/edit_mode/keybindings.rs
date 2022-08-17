use crossterm::event::{KeyEventKind, KeyEventState};

use {
    crate::{enums::ReedlineEvent, EditCommand},
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombination {
    pub modifier: KeyModifiers,
    pub key_code: KeyCode,
    pub kind: KeyEventKind,
    pub state: KeyEventState,
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
    ///
    /// # Panics
    ///
    /// If `comamnd` is an empty [`ReedlineEvent::UntilFound`]
    pub fn add_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        kind: KeyEventKind,
        state: KeyEventState,
        command: ReedlineEvent,
    ) {
        if let ReedlineEvent::UntilFound(subcommands) = &command {
            assert!(
                !subcommands.is_empty(),
                "UntilFound should contain a series of potential events to handle"
            );
        }

        let key_combo = KeyCombination {
            modifier,
            key_code,
            kind,
            state,
        };
        self.bindings.insert(key_combo, command);
    }

    /// Find a keybinding based on the modifier and keycode
    pub fn find_binding(
        &self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        kind: KeyEventKind,
        state: KeyEventState,
    ) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination {
            modifier,
            key_code,
            kind,
            state,
        };
        self.bindings.get(&key_combo).cloned()
    }

    /// Remove a keybinding
    ///
    /// Returns `Some(ReedlineEvent)` if the keycombination was previously bound to a particular [`ReedlineEvent`]
    pub fn remove_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        kind: KeyEventKind,
        state: KeyEventState,
    ) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination {
            modifier,
            key_code,
            kind,
            state,
        };
        self.bindings.remove(&key_combo)
    }

    /// Get assigned keybindings
    pub fn get_keybindings(&self) -> &HashMap<KeyCombination, ReedlineEvent> {
        &self.bindings
    }
}

pub fn edit_bind(command: EditCommand) -> ReedlineEvent {
    ReedlineEvent::Edit(vec![command])
}

/// Add the basic special keybindings
///
/// `Ctrl-C`, `Ctrl-D`, `Ctrl-O`, `Ctrl-R`
/// + `Esc`
/// + `Ctrl-O` to open the external editor
pub fn add_common_control_bindings(kb: &mut Keybindings) {
    use KeyCode as KC;
    use KeyModifiers as KM;

    kb.add_binding(
        KM::NONE,
        KC::Esc,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::Esc,
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('c'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::CtrlC,
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('d'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::CtrlD,
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('l'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::ClearScreen,
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('r'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::SearchHistory,
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('o'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::OpenEditor,
    );
}
/// Add the arrow navigation and its `Ctrl` variants
pub fn add_common_navigation_bindings(kb: &mut Keybindings) {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    // Arrow keys without modifier
    kb.add_binding(
        KM::NONE,
        KC::Up,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuUp, ReedlineEvent::Up]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Down,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuDown, ReedlineEvent::Down]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Left,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuLeft, ReedlineEvent::Left]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Right,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::MenuRight,
            ReedlineEvent::Right,
        ]),
    );

    // Ctrl Left and Right
    kb.add_binding(
        KM::CONTROL,
        KC::Left,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveWordLeft),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Right,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight),
        ]),
    );
    // Home/End & ctrl+a/ctrl+e
    kb.add_binding(
        KM::NONE,
        KC::Home,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveToLineStart),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('a'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveToLineStart),
    );
    kb.add_binding(
        KM::NONE,
        KC::End,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            edit_bind(EC::MoveToLineEnd),
        ]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('e'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            edit_bind(EC::MoveToLineEnd),
        ]),
    );
    // Ctrl Home/End
    kb.add_binding(
        KM::CONTROL,
        KC::Home,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveToStart),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::End,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveToEnd),
    );
    // EMACS arrows
    kb.add_binding(
        KM::CONTROL,
        KC::Char('p'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuUp, ReedlineEvent::Up]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('n'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuDown, ReedlineEvent::Down]),
    );
}

/// Add basic functionality to edit
///
/// `Delete`, `Backspace` and the basic variants do delete words
pub fn add_common_edit_bindings(kb: &mut Keybindings) {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;
    kb.add_binding(
        KM::NONE,
        KC::Backspace,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::Backspace),
    );
    kb.add_binding(
        KM::NONE,
        KC::Delete,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::Delete),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Backspace,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::BackspaceWord),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Delete,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::DeleteWord),
    );
    // Base commands should not affect cut buffer
    kb.add_binding(
        KM::CONTROL,
        KC::Char('h'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::Backspace),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('w'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::BackspaceWord),
    );
}
