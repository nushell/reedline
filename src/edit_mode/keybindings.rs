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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeySequence(pub Vec<KeyCombination>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySequenceMatch {
    Exact(ReedlineEvent),
    Prefix,
    ExactAndPrefix(ReedlineEvent),
    NoMatch,
}

#[derive(Debug, Default, Clone)]
pub struct KeySequenceState {
    buffer: Vec<KeyCombination>,
    pending_exact: Option<(usize, ReedlineEvent)>,
}

/// Main definition of editor keybindings
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Keybindings {
    /// Defines a keybinding for a reedline event
    pub bindings: HashMap<KeyCombination, ReedlineEvent>,
    /// Defines a key sequence binding for a reedline event
    #[serde(default)]
    pub sequence_bindings: HashMap<KeySequence, ReedlineEvent>,
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
            sequence_bindings: HashMap::new(),
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
    /// If `command` is an empty [`ReedlineEvent::UntilFound`]
    pub fn add_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        command: ReedlineEvent,
    ) {
        if let ReedlineEvent::UntilFound(subcommands) = &command {
            assert!(
                !subcommands.is_empty(),
                "UntilFound should contain a series of potential events to handle"
            );
        }

        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.insert(key_combo, command);
    }

    /// Adds a key sequence binding
    ///
    /// # Panics
    ///
    /// If `sequence` is empty or `command` is an empty [`ReedlineEvent::UntilFound`]
    pub fn add_sequence_binding(&mut self, sequence: Vec<KeyCombination>, command: ReedlineEvent) {
        assert!(
            !sequence.is_empty(),
            "Key sequence must contain at least one key"
        );

        if let ReedlineEvent::UntilFound(subcommands) = &command {
            assert!(
                !subcommands.is_empty(),
                "UntilFound should contain a series of potential events to handle"
            );
        }

        self.sequence_bindings
            .insert(KeySequence(sequence), command);
    }

    /// Find a keybinding based on the modifier and keycode
    pub fn find_binding(&self, modifier: KeyModifiers, key_code: KeyCode) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.get(&key_combo).cloned()
    }

    /// Find how a key sequence matches existing bindings
    pub fn sequence_match(&self, sequence: &[KeyCombination]) -> KeySequenceMatch {
        if sequence.is_empty() || self.sequence_bindings.is_empty() {
            return KeySequenceMatch::NoMatch;
        }

        let exact = self
            .sequence_bindings
            .get(&KeySequence(sequence.to_vec()))
            .cloned();

        let is_prefix = self.sequence_bindings.keys().any(|key_sequence| {
            key_sequence.0.len() > sequence.len() && key_sequence.0[..sequence.len()] == *sequence
        });

        match (exact, is_prefix) {
            (Some(event), true) => KeySequenceMatch::ExactAndPrefix(event),
            (Some(event), false) => KeySequenceMatch::Exact(event),
            (None, true) => KeySequenceMatch::Prefix,
            (None, false) => KeySequenceMatch::NoMatch,
        }
    }

    /// Remove a keybinding
    ///
    /// Returns `Some(ReedlineEvent)` if the key combination was previously bound to a particular [`ReedlineEvent`]
    pub fn remove_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.remove(&key_combo)
    }

    /// Remove a key sequence binding
    ///
    /// Returns `Some(ReedlineEvent)` if the key sequence was previously bound to a particular [`ReedlineEvent`]
    pub fn remove_sequence_binding(
        &mut self,
        sequence: Vec<KeyCombination>,
    ) -> Option<ReedlineEvent> {
        self.sequence_bindings.remove(&KeySequence(sequence))
    }

    /// Get assigned keybindings
    pub fn get_keybindings(&self) -> &HashMap<KeyCombination, ReedlineEvent> {
        &self.bindings
    }

    /// Get assigned sequence bindings
    pub fn get_sequence_keybindings(&self) -> &HashMap<KeySequence, ReedlineEvent> {
        &self.sequence_bindings
    }
}

impl KeySequenceState {
    pub const fn new() -> Self {
        Self {
            buffer: Vec::new(),
            pending_exact: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.pending_exact = None;
    }

    pub fn process_combo<F>(
        &mut self,
        keybindings: &Keybindings,
        combo: KeyCombination,
        mut fallback: F,
    ) -> Option<ReedlineEvent>
    where
        F: FnMut(KeyCombination) -> ReedlineEvent,
    {
        if keybindings.sequence_bindings.is_empty() {
            self.clear();
            return Some(fallback(combo));
        }

        self.buffer.push(combo);

        let mut events = Vec::new();

        loop {
            match keybindings.sequence_match(&self.buffer) {
                KeySequenceMatch::Exact(event) => {
                    self.buffer.clear();
                    self.pending_exact = None;
                    events.push(event);
                    break;
                }
                KeySequenceMatch::ExactAndPrefix(event) => {
                    self.pending_exact = Some((self.buffer.len(), event));
                    break;
                }
                KeySequenceMatch::Prefix => {
                    self.pending_exact = None;
                    break;
                }
                KeySequenceMatch::NoMatch => {
                    if let Some((pending_len, pending_event)) = self.pending_exact.take() {
                        let remaining = if pending_len < self.buffer.len() {
                            self.buffer.split_off(pending_len)
                        } else {
                            Vec::new()
                        };
                        self.buffer.clear();
                        self.buffer = remaining;
                        events.push(pending_event);
                        if self.buffer.is_empty() {
                            break;
                        }
                        continue;
                    }

                    let flushed = self.buffer.remove(0);
                    let event = fallback(flushed);
                    if !matches!(event, ReedlineEvent::None) {
                        events.push(event);
                    }

                    if self.buffer.is_empty() {
                        break;
                    }
                }
            }
        }

        let mut events: Vec<ReedlineEvent> = events
            .into_iter()
            .filter(|event| !matches!(event, ReedlineEvent::None))
            .collect();

        if events.is_empty() {
            return None;
        }

        if events.len() == 1 {
            return Some(events.remove(0));
        }

        Some(ReedlineEvent::Multiple(events))
    }

    pub fn flush<F>(&mut self, mut fallback: F) -> Option<ReedlineEvent>
    where
        F: FnMut(KeyCombination) -> ReedlineEvent,
    {
        if self.buffer.is_empty() {
            self.pending_exact = None;
            return None;
        }

        if let Some((pending_len, pending_event)) = self.pending_exact.take() {
            let remaining = if pending_len < self.buffer.len() {
                self.buffer.split_off(pending_len)
            } else {
                Vec::new()
            };
            self.buffer.clear();

            let mut events = vec![pending_event];
            for combo in remaining {
                let event = fallback(combo);
                if !matches!(event, ReedlineEvent::None) {
                    events.push(event);
                }
            }

            return match events.len() {
                0 => None,
                1 => Some(events.remove(0)),
                _ => Some(ReedlineEvent::Multiple(events)),
            };
        }

        let mut events = Vec::new();
        for combo in self.buffer.drain(..) {
            let event = fallback(combo);
            if !matches!(event, ReedlineEvent::None) {
                events.push(event);
            }
        }

        match events.len() {
            0 => None,
            1 => Some(events.remove(0)),
            _ => Some(ReedlineEvent::Multiple(events)),
        }
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

    kb.add_binding(KM::NONE, KC::Esc, ReedlineEvent::Esc);
    kb.add_binding(KM::CONTROL, KC::Char('c'), ReedlineEvent::CtrlC);
    kb.add_binding(KM::CONTROL, KC::Char('d'), ReedlineEvent::CtrlD);
    kb.add_binding(KM::CONTROL, KC::Char('l'), ReedlineEvent::ClearScreen);
    kb.add_binding(KM::CONTROL, KC::Char('r'), ReedlineEvent::SearchHistory);
    kb.add_binding(KM::CONTROL, KC::Char('o'), ReedlineEvent::OpenEditor);
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
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuUp, ReedlineEvent::Up]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Down,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuDown, ReedlineEvent::Down]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Left,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuLeft, ReedlineEvent::Left]),
    );
    kb.add_binding(
        KM::NONE,
        KC::Right,
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
        edit_bind(EC::MoveWordLeft { select: false }),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Right,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight { select: false }),
        ]),
    );
    // Home/End & ctrl+a/ctrl+e
    kb.add_binding(
        KM::NONE,
        KC::Home,
        edit_bind(EC::MoveToLineStart { select: false }),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('a'),
        edit_bind(EC::MoveToLineStart { select: false }),
    );
    kb.add_binding(
        KM::NONE,
        KC::End,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            edit_bind(EC::MoveToLineEnd { select: false }),
        ]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('e'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            edit_bind(EC::MoveToLineEnd { select: false }),
        ]),
    );
    // Ctrl Home/End
    kb.add_binding(
        KM::CONTROL,
        KC::Home,
        edit_bind(EC::MoveToStart { select: false }),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::End,
        edit_bind(EC::MoveToEnd { select: false }),
    );
    // EMACS arrows
    kb.add_binding(
        KM::CONTROL,
        KC::Char('p'),
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuUp, ReedlineEvent::Up]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('n'),
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
    kb.add_binding(KM::NONE, KC::Backspace, edit_bind(EC::Backspace));
    kb.add_binding(KM::NONE, KC::Delete, edit_bind(EC::Delete));
    kb.add_binding(KM::CONTROL, KC::Backspace, edit_bind(EC::BackspaceWord));
    kb.add_binding(KM::CONTROL, KC::Delete, edit_bind(EC::DeleteWord));
    // Base commands should not affect cut buffer
    kb.add_binding(KM::CONTROL, KC::Char('h'), edit_bind(EC::Backspace));
    kb.add_binding(KM::CONTROL, KC::Char('w'), edit_bind(EC::BackspaceWord));
    #[cfg(feature = "system_clipboard")]
    kb.add_binding(
        KM::CONTROL | KM::SHIFT,
        KC::Char('x'),
        edit_bind(EC::CutSelectionSystem),
    );
    #[cfg(feature = "system_clipboard")]
    kb.add_binding(
        KM::CONTROL | KM::SHIFT,
        KC::Char('c'),
        edit_bind(EC::CopySelectionSystem),
    );
    #[cfg(feature = "system_clipboard")]
    kb.add_binding(
        KM::CONTROL | KM::SHIFT,
        KC::Char('v'),
        edit_bind(EC::PasteSystem),
    );
    kb.add_binding(KM::ALT, KC::Enter, edit_bind(EC::InsertNewline));
    kb.add_binding(KM::SHIFT, KC::Enter, edit_bind(EC::InsertNewline));
    kb.add_binding(KM::CONTROL, KC::Char('j'), ReedlineEvent::Enter);
}

pub fn add_common_selection_bindings(kb: &mut Keybindings) {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    kb.add_binding(
        KM::SHIFT,
        KC::Left,
        edit_bind(EC::MoveLeft { select: true }),
    );
    kb.add_binding(
        KM::SHIFT,
        KC::Right,
        edit_bind(EC::MoveRight { select: true }),
    );
    kb.add_binding(
        KM::SHIFT | KM::CONTROL,
        KC::Left,
        edit_bind(EC::MoveWordLeft { select: true }),
    );
    kb.add_binding(
        KM::SHIFT | KM::CONTROL,
        KC::Right,
        edit_bind(EC::MoveWordRight { select: true }),
    );
    kb.add_binding(
        KM::SHIFT,
        KC::End,
        edit_bind(EC::MoveToLineEnd { select: true }),
    );
    kb.add_binding(
        KM::SHIFT | KM::CONTROL,
        KC::End,
        edit_bind(EC::MoveToEnd { select: true }),
    );
    kb.add_binding(
        KM::SHIFT,
        KC::Home,
        edit_bind(EC::MoveToLineStart { select: true }),
    );
    kb.add_binding(
        KM::SHIFT | KM::CONTROL,
        KC::Home,
        edit_bind(EC::MoveToStart { select: true }),
    );
    kb.add_binding(
        KM::CONTROL | KM::SHIFT,
        KC::Char('a'),
        edit_bind(EC::SelectAll),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditCommand;

    fn combo(c: char) -> KeyCombination {
        KeyCombination {
            modifier: KeyModifiers::NONE,
            key_code: KeyCode::Char(c),
        }
    }

    fn fallback(combo: KeyCombination) -> ReedlineEvent {
        match combo.key_code {
            KeyCode::Char(c) => ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)]),
            _ => ReedlineEvent::None,
        }
    }

    #[test]
    fn sequence_match_detects_prefix_and_exact() {
        let mut keybindings = Keybindings::new();
        keybindings.add_sequence_binding(vec![combo('j'), combo('j')], ReedlineEvent::Esc);

        assert!(matches!(
            keybindings.sequence_match(&[combo('j')]),
            KeySequenceMatch::Prefix
        ));
        assert!(matches!(
            keybindings.sequence_match(&[combo('j'), combo('j')]),
            KeySequenceMatch::Exact(ReedlineEvent::Esc)
        ));
    }

    #[test]
    fn sequence_state_emits_on_match() {
        let mut keybindings = Keybindings::new();
        keybindings.add_sequence_binding(vec![combo('j'), combo('j')], ReedlineEvent::Esc);

        let mut state = KeySequenceState::default();
        let first = state.process_combo(&keybindings, combo('j'), fallback);
        assert_eq!(first, None);

        let second = state.process_combo(&keybindings, combo('j'), fallback);
        assert_eq!(second, Some(ReedlineEvent::Esc));
    }

    #[test]
    fn sequence_state_flushes_on_miss() {
        let mut keybindings = Keybindings::new();
        keybindings.add_sequence_binding(vec![combo('j'), combo('j')], ReedlineEvent::Esc);

        let mut state = KeySequenceState::default();
        let _ = state.process_combo(&keybindings, combo('j'), fallback);
        let second = state.process_combo(&keybindings, combo('k'), fallback);

        assert_eq!(
            second,
            Some(ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')]),
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('k')]),
            ]))
        );
    }

    #[test]
    fn sequence_state_flushes_pending_on_timeout() {
        let mut keybindings = Keybindings::new();
        keybindings.add_sequence_binding(vec![combo('j'), combo('j')], ReedlineEvent::Esc);

        let mut state = KeySequenceState::default();
        let _ = state.process_combo(&keybindings, combo('j'), fallback);
        let flushed = state.flush(fallback);

        assert_eq!(
            flushed,
            Some(ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')]))
        );
    }
}
