use {
    crate::{enums::ReedlineEvent, EditCommand},
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

/// Key combination consisting of modifier(s) and a key code.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombination {
    /// Key modifiers (e.g., Ctrl, Alt, Shift)
    pub modifier: KeyModifiers,
    /// Key code (e.g., Char('a'), Enter)
    pub key_code: KeyCode,
}

/// Sequence of key combinations.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeySequence(pub Vec<KeyCombination>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySequenceMatch {
    Exact(ReedlineEvent),
    Prefix,
    ExactAndPrefix(ReedlineEvent),
    NoMatch,
}

/// State used to track partial key sequence matches.
#[derive(Debug, Default, Clone)]
pub struct KeySequenceState {
    buffer: Vec<KeyCombination>,
    /// Stores an exact match that is also a prefix of a longer sequence.
    /// If the longer sequence does not materialize, we emit this saved event
    /// and continue processing any remaining buffered keys.
    pending_exact: Option<(usize, ReedlineEvent)>,
}

/// Resolution result for processing a key sequence.
#[derive(Debug, Default, Clone)]
pub struct SequenceResolution {
    /// Events emitted from matched sequences.
    pub events: Vec<ReedlineEvent>,
    /// Key combinations to flush through fallback handling.
    pub combos: Vec<KeyCombination>,
}

impl SequenceResolution {
    pub fn into_event<F>(self, mut fallback: F) -> Option<ReedlineEvent>
    where
        F: FnMut(KeyCombination) -> ReedlineEvent,
    {
        let mut events = Vec::new();
        for event in self.events {
            append_event(&mut events, event);
        }

        for combo in self.combos {
            append_event(&mut events, fallback(combo));
        }

        combine_events(events)
    }
}

/// Main definition of editor keybindings
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Keybindings {
    /// Defines a keybinding for a reedline event
    pub bindings: HashMap<KeyCombination, ReedlineEvent>,
    /// Defines a key sequence binding for a reedline event
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

    pub fn process_combo(
        &mut self,
        keybindings: &Keybindings,
        combo: KeyCombination,
    ) -> SequenceResolution {
        if keybindings.sequence_bindings.is_empty() {
            self.clear();
            return SequenceResolution {
                combos: vec![combo],
                ..SequenceResolution::default()
            };
        }

        self.buffer.push(combo);
        let mut resolution = SequenceResolution::default();

        loop {
            match self.process_step(keybindings, &mut resolution) {
                StepOutcome::Continue => continue,
                StepOutcome::EmitDone | StepOutcome::Pending | StepOutcome::Done => break,
            }
        }

        if self.buffer.is_empty() {
            self.pending_exact = None;
        }
        resolution
    }

    pub fn flush_with_combos(&mut self) -> SequenceResolution {
        if self.buffer.is_empty() {
            return SequenceResolution::default();
        }

        let mut resolution = SequenceResolution::default();

        if !self.flush_pending_exact(&mut resolution) {
            resolution.combos = std::mem::take(&mut self.buffer);
        }

        self.pending_exact = None;
        resolution
    }

    fn flush_pending_exact(&mut self, resolution: &mut SequenceResolution) -> bool {
        let Some((pending_len, pending_event)) = self.pending_exact.take() else {
            return false;
        };

        let pending_len = pending_len.min(self.buffer.len());
        self.buffer.drain(..pending_len);
        resolution.events.push(pending_event);
        true
    }

    fn process_step(
        &mut self,
        keybindings: &Keybindings,
        resolution: &mut SequenceResolution,
    ) -> StepOutcome {
        match keybindings.sequence_match(&self.buffer) {
            KeySequenceMatch::Exact(event) => {
                self.buffer.clear();
                self.pending_exact = None;
                resolution.events.push(event);
                StepOutcome::EmitDone
            }
            KeySequenceMatch::ExactAndPrefix(event) => {
                self.pending_exact = Some((self.buffer.len(), event));
                StepOutcome::Pending
            }
            KeySequenceMatch::Prefix => {
                self.pending_exact = None;
                StepOutcome::Pending
            }
            // User input does not match any sequence; flush buffered keys.
            KeySequenceMatch::NoMatch => {
                // If we previously saw an exact match that was also a prefix, emit it now
                // and keep any trailing keys for further processing.
                if self.flush_pending_exact(resolution) {
                    if self.buffer.is_empty() {
                        return StepOutcome::Done;
                    }
                    return StepOutcome::Continue;
                }

                // Otherwise, drop the oldest key and replay it through fallback handling.
                // NOTE: This is O(n), but sequence buffers are expected to be short.
                let flushed = self.buffer.remove(0);
                resolution.combos.push(flushed);
                if self.buffer.is_empty() {
                    StepOutcome::Done
                } else {
                    StepOutcome::Continue
                }
            }
        }
    }
}

enum StepOutcome {
    /// Shifted buffered input; continue resolving remaining keys.
    Continue,
    /// Current buffer is a valid prefix; wait for more input.
    Pending,
    /// Emitted an exact match; stop processing this step.
    EmitDone,
    /// Buffer emptied with no further input to process.
    Done,
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

fn append_event(events: &mut Vec<ReedlineEvent>, event: ReedlineEvent) {
    match event {
        ReedlineEvent::None => {}
        ReedlineEvent::Multiple(mut inner) => events.append(&mut inner),
        other => events.push(other),
    }
}

fn combine_events(mut events: Vec<ReedlineEvent>) -> Option<ReedlineEvent> {
    if events.is_empty() {
        return None;
    }

    if events.len() == 1 {
        return Some(events.remove(0));
    }

    Some(ReedlineEvent::Multiple(events))
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
        let first = state.process_combo(&keybindings, combo('j'));
        assert_eq!(first.into_event(fallback), None);

        let second = state.process_combo(&keybindings, combo('j'));
        assert_eq!(second.into_event(fallback), Some(ReedlineEvent::Esc));
    }

    #[test]
    fn sequence_state_flushes_on_miss() {
        let mut keybindings = Keybindings::new();
        keybindings.add_sequence_binding(vec![combo('j'), combo('j')], ReedlineEvent::Esc);

        let mut state = KeySequenceState::default();
        let _ = state.process_combo(&keybindings, combo('j'));
        let second = state.process_combo(&keybindings, combo('k'));

        assert_eq!(
            second.into_event(fallback),
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
        let _ = state.process_combo(&keybindings, combo('j'));
        let flushed = state.flush_with_combos();

        assert_eq!(
            flushed.into_event(fallback),
            Some(ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')]))
        );
    }
}
