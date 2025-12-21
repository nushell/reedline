use {
    crate::{enums::ReedlineEvent, EditCommand},
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

/// Representation of a key combination: modifier + key code
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombination {
    /// Modifier keys
    pub modifier: KeyModifiers,
    /// The key code
    pub key_code: KeyCode,
}

/// Main definition of editor keybindings
#[derive(Clone, Debug)]
pub struct Keybindings {
    /// Trie mapping key combination sequences to their corresponding events.
    root: KeyBindingNode,
}

/// Target that a key combination may be bound to.
pub enum KeyBindingTarget {
    /// Indicates a binding to an event.
    Event(ReedlineEvent),
    /// Indicates that this is a prefix to other bindings.
    ChordPrefix,
}

// TODO: Implement Serialize for Keybindings
// TODO: Implement Deserialize for Keybindings

/// Trie node that represents a key combination's binding. The key
/// combination may *only* be bound to an event, or may be a
/// strict prefix of a chord of key combinations.
#[derive(Clone, Debug)]
enum KeyBindingNode {
    /// Indicates a binding to an event.
    Event(ReedlineEvent),
    /// Indicates that this is a prefix to other bindings.
    Prefix(HashMap<KeyCombination, KeyBindingNode>),
}

impl KeyBindingNode {
    fn new_prefix() -> Self {
        KeyBindingNode::Prefix(HashMap::new())
    }
}

impl Default for Keybindings {
    fn default() -> Self {
        Self::new()
    }
}

impl Keybindings {
    /// Returns a new, empty keybinding set
    pub fn new() -> Self {
        Self {
            root: KeyBindingNode::new_prefix(),
        }
    }

    /// Returns a new, empty keybinding set
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

        self.add_sequence_binding(&[key_combo], command);
    }

    /// Binds a sequence of key combinations to an event. This allows binding a single
    /// key combination, as well as binding a chord of multiple key combinations.
    pub fn add_sequence_binding(&mut self, sequence: &[KeyCombination], event: ReedlineEvent) {
        if sequence.len() == 0 {
            return;
        }

        let mut current_target = &mut self.root;

        for i in 0..(sequence.len() - 1) {
            match current_target {
                KeyBindingNode::Prefix(ref mut map) => {
                    let combo = &sequence[i];

                    current_target = map
                        .entry(combo.clone())
                        .or_insert_with(KeyBindingNode::new_prefix);
                }
                KeyBindingNode::Event(_) => {
                    // Overwrite existing event binding with a prefix
                    *current_target = KeyBindingNode::new_prefix();
                }
            }
        }

        let final_combo = &sequence[sequence.len() - 1];

        match current_target {
            KeyBindingNode::Prefix(ref mut map) => {
                map.insert(final_combo.clone(), KeyBindingNode::Event(event));
            }
            KeyBindingNode::Event(_) => {
                // Overwrite existing event binding with a prefix initialized with the event.
                let prefix = KeyBindingNode::Prefix(HashMap::from([(
                    final_combo.clone(),
                    KeyBindingNode::Event(event),
                )]));
                *current_target = prefix;
            }
        }
    }

    /// Find a keybinding based on the modifier and keycode.
    pub fn find_binding(&self, modifier: KeyModifiers, key_code: KeyCode) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination { modifier, key_code };
        self.find_sequence_binding(&[key_combo]).and_then(|target| {
            if let KeyBindingTarget::Event(event) = target {
                Some(event)
            } else {
                None
            }
        })
    }

    /// Find a keybinding based on a sequence of key combinations.
    ///
    /// Returns `Some(KeyBindingTarget::Event(ReedlineEvent))` if the sequence is bound to a
    /// particular [`ReedlineEvent`].
    ///
    /// Returns `Some(KeyBindingTarget::ChordPrefix)` if the sequence is a strict prefix
    /// of other bindings.
    ///
    /// Returns `None` if the sequence is not bound.
    pub fn find_sequence_binding(&self, sequence: &[KeyCombination]) -> Option<KeyBindingTarget> {
        let mut current_target = &self.root;

        for i in 0..sequence.len() {
            match current_target {
                KeyBindingNode::Prefix(map) => {
                    if let Some(next_target) = map.get(&sequence[i]) {
                        current_target = next_target;
                    } else {
                        return None;
                    }
                }
                KeyBindingNode::Event(_) => return None,
            }
        }

        match current_target {
            KeyBindingNode::Prefix(_) => Some(KeyBindingTarget::ChordPrefix),
            KeyBindingNode::Event(event) => Some(KeyBindingTarget::Event(event.clone())),
        }
    }

    /// Remove a single-key keybinding. If the indicated key combination is a strict prefix
    /// of chord bindings, those latter bindings are preserved.
    ///
    /// Returns `Some(ReedlineEvent)` if the key combination was previously bound to a particular [`ReedlineEvent`]
    pub fn remove_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
    ) -> Option<ReedlineEvent> {
        let key_combo = KeyCombination { modifier, key_code };
        self.remove_sequence_binding(&[key_combo])
    }

    /// Unbind a sequence of key combinations. If the given sequence is a strict prefix
    /// of other bindings, those bindings are preserved.
    ///
    /// Returns `Some(ReedlineEvent)` if the sequence was previously bound to a particular [`ReedlineEvent`]
    pub fn remove_sequence_binding(
        &mut self,
        sequence: &[KeyCombination],
    ) -> Option<ReedlineEvent> {
        let mut current_target = &mut self.root;

        if sequence.len() == 0 {
            return None;
        }

        for i in 0..(sequence.len() - 1) {
            match current_target {
                KeyBindingNode::Prefix(map) => {
                    if let Some(next_target) = map.get_mut(&sequence[i]) {
                        current_target = next_target;
                    } else {
                        return None;
                    }
                }
                KeyBindingNode::Event(_) => return None,
            }
        }

        let final_combo = &sequence[sequence.len() - 1];

        match current_target {
            KeyBindingNode::Prefix(map) => {
                if map
                    .get(final_combo)
                    .is_none_or(|target| matches!(target, KeyBindingNode::Prefix(_)))
                {
                    None
                } else if let Some(KeyBindingNode::Event(old_event)) = map.remove(final_combo) {
                    Some(old_event)
                } else {
                    None
                }
            }
            KeyBindingNode::Event(_) => None,
        }
    }

    /// Get assigned single-key keybindings.
    pub fn get_keybindings(&self) -> impl IntoIterator<Item = (&KeyCombination, &ReedlineEvent)> {
        self.get_sequence_bindings()
            .into_iter()
            .filter_map(|(seq, event)| {
                if let [first, ..] = seq {
                    Some((first, event))
                } else {
                    None
                }
            })
    }

    /// Get all bindings for key sequences, including single-key bindings and chords.
    pub fn get_sequence_bindings(
        &self,
    ) -> impl IntoIterator<Item = (&[KeyCombination], &ReedlineEvent)> {
        // TODO
        []
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
