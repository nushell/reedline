use {
    crate::{enums::ReedlineEvent, EditCommand},
    crossterm::event::{KeyCode, KeyModifiers},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

/// One keypress: a key code plus the modifiers held with it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyCombination {
    /// Modifier keys held for this key.
    pub modifier: KeyModifiers,
    /// The key itself.
    pub key_code: KeyCode,
}

/// One node of the sequence trie.
///
/// A node may carry an event *and* children: `j` can be a complete binding
/// while `j j` continues the sequence.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SequenceNode {
    event: Option<ReedlineEvent>,
    children: HashMap<KeyCombination, SequenceNode>,
}

/// A prefix trie over multi-key sequences.
///
/// Single-key bindings stay in [`Keybindings::bindings`], so the long-standing
/// `add_binding` / `find_binding` API is untouched. A sequence of length one
/// given to [`Keybindings::add_binding_sequence`] is stored there too.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SequenceTrie {
    root: SequenceNode,
}

impl SequenceTrie {
    fn insert(&mut self, keys: &[KeyCombination], event: ReedlineEvent) {
        let mut node = &mut self.root;
        for key in keys {
            node = node.children.entry(key.clone()).or_default();
        }
        node.event = Some(event);
    }

    fn lookup(&self, keys: &[KeyCombination]) -> SequenceLookup {
        let mut node = &self.root;
        for key in keys {
            match node.children.get(key) {
                Some(child) => node = child,
                None => {
                    return SequenceLookup {
                        exact: None,
                        is_prefix: false,
                    }
                }
            }
        }
        SequenceLookup {
            exact: node.event.clone(),
            is_prefix: !node.children.is_empty(),
        }
    }

    fn is_empty(&self) -> bool {
        self.root.children.is_empty()
    }
}

/// What the trie knows about a key path: whether it is a complete binding and
/// whether longer sequences continue through it.
#[derive(Debug, Clone)]
pub(crate) struct SequenceLookup {
    pub(crate) exact: Option<ReedlineEvent>,
    pub(crate) is_prefix: bool,
}

/// Main definition of editor keybindings
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Keybindings {
    /// Defines a keybinding for a reedline event
    pub bindings: HashMap<KeyCombination, ReedlineEvent>,
    /// Multi-key sequences. Kept out of the public single-key map so direct
    /// edits to `bindings` cannot leave a stale trie behind.
    #[serde(default, skip_serializing_if = "SequenceTrie::is_empty")]
    sequences: SequenceTrie,
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
            sequences: SequenceTrie::default(),
        }
    }

    /// Defines an empty keybinding object
    pub fn empty() -> Self {
        Self::new()
    }

    /// Adds a keybinding
    ///
    /// An empty [`ReedlineEvent::UntilFound`] is accepted; it never matches
    /// anything and behaves like an unbound key.
    pub fn add_binding(
        &mut self,
        modifier: KeyModifiers,
        key_code: KeyCode,
        command: ReedlineEvent,
    ) {
        let key_combo = KeyCombination { modifier, key_code };
        self.bindings.insert(key_combo, command);
    }

    /// Adds a multi-key sequence binding.
    ///
    /// `keys` are matched in order. A single key is accepted and stored exactly
    /// like [`add_binding`](Self::add_binding), so the single-key lookup
    /// behaviour is unchanged. An empty slice is a no-op.
    ///
    /// A key may be both a binding and the start of a longer sequence: binding
    /// `j` with `add_binding` and `j j` here makes `j` wait for a possible
    /// second `j`; the timeout or an unrelated key falls back to the `j`
    /// binding.
    pub fn add_binding_sequence(&mut self, keys: &[KeyCombination], command: ReedlineEvent) {
        match keys {
            [] => {}
            [key] => {
                self.bindings.insert(key.clone(), command);
            }
            keys => self.sequences.insert(keys, command),
        }
    }

    /// Find a multi-key sequence binding, exactly.
    ///
    /// A one-key slice reads the ordinary single-key table.
    pub fn find_sequence_binding(&self, keys: &[KeyCombination]) -> Option<ReedlineEvent> {
        match keys {
            [] => None,
            [key] => self.bindings.get(key).cloned(),
            keys => self.sequences.lookup(keys).exact,
        }
    }

    /// Whether `keys` is a proper prefix of at least one longer sequence.
    pub fn is_sequence_prefix(&self, keys: &[KeyCombination]) -> bool {
        !keys.is_empty() && self.sequences.lookup(keys).is_prefix
    }

    /// The trie view used by [`SequenceResolver`](super::SequenceResolver).
    pub(crate) fn lookup_sequence(&self, keys: &[KeyCombination]) -> SequenceLookup {
        self.sequences.lookup(keys)
    }

    /// The longest multi-key sequence at the start of `keys`, with its length.
    ///
    /// Used when a pending prefix is flushed: a complete sequence inside the
    /// prefix wins over replaying its keys one by one.
    pub(crate) fn longest_sequence_prefix(
        &self,
        keys: &[KeyCombination],
    ) -> Option<(usize, ReedlineEvent)> {
        (2..=keys.len()).rev().find_map(|len| {
            self.sequences
                .lookup(&keys[..len])
                .exact
                .map(|event| (len, event))
        })
    }

    /// Find a keybinding based on the modifier and keycode.
    ///
    /// If a shifted non-character key is unbound, try again without Shift.
    pub fn find_binding(&self, modifier: KeyModifiers, key_code: KeyCode) -> Option<ReedlineEvent> {
        let mut key_combo = KeyCombination { modifier, key_code };
        self.bindings
            .get(&key_combo)
            .or_else(|| {
                if modifier.contains(KeyModifiers::SHIFT) && !matches!(key_code, KeyCode::Char(_)) {
                    key_combo.modifier.remove(KeyModifiers::SHIFT);
                    self.bindings.get(&key_combo)
                } else {
                    None
                }
            })
            .cloned()
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

    // Jump to the start/end of the buffer
    kb.add_binding(KM::ALT, KC::Char('<'), ReedlineEvent::ToStart);
    kb.add_binding(KM::ALT, KC::Char('>'), ReedlineEvent::ToEnd);
    // For kitty keyboard protocol
    kb.add_binding(KM::SHIFT | KM::ALT, KC::Char(','), ReedlineEvent::ToStart);
    kb.add_binding(KM::SHIFT | KM::ALT, KC::Char('.'), ReedlineEvent::ToEnd);
}

/// Rebind the navigation set for a mode that holds a selection open (vi
/// visual, helix select), on top of [`add_common_navigation_bindings`].
///
/// Each key extends like the modal motion it stands for instead of moving,
/// which would drop the selection and start a new one. Up/Down never reach
/// menus or history, since history traversal would replace the buffer the
/// selection is anchored in, and no key accepts a history hint, since that
/// inserts text.
///
/// `left` and `right` are the grapheme steps, passed in since the two
/// machines lower `h`/`l` to different commands and a key should equal its
/// own twin. `Backspace` follows `left`, as it follows `h` in normal.
pub(crate) fn add_extending_navigation_bindings(
    kb: &mut Keybindings,
    left: EditCommand,
    right: EditCommand,
) {
    use crate::{Direction as D, MotionTarget as MT, WordEdge, WordKind};
    use KeyCode as KC;
    use KeyModifiers as KM;

    let extend = |target: MT| edit_bind(EditCommand::Extend(target));

    // `h`/`l` by grapheme, with Backspace following `h`.
    kb.add_binding(KM::NONE, KC::Left, edit_bind(left.clone()));
    kb.add_binding(KM::NONE, KC::Right, edit_bind(right));
    kb.add_binding(KM::NONE, KC::Backspace, edit_bind(left));
    let word = |direction: D| MT::Word {
        kind: WordKind::Word,
        edge: WordEdge::Start,
        direction,
    };
    let line_up = || edit_bind(EditCommand::MoveLineUp { select: true });
    let line_down = || edit_bind(EditCommand::MoveLineDown { select: true });

    // `k`/`j` by line, with the emacs-style aliases following them.
    kb.add_binding(KM::NONE, KC::Up, line_up());
    kb.add_binding(KM::NONE, KC::Down, line_down());
    kb.add_binding(KM::CONTROL, KC::Char('p'), line_up());
    kb.add_binding(KM::CONTROL, KC::Char('n'), line_down());
    // Word chords, the `b`/`w` twins.
    kb.add_binding(KM::CONTROL, KC::Left, extend(word(D::Backward)));
    kb.add_binding(KM::CONTROL, KC::Right, extend(word(D::Forward)));
    // Line edges on Home/End and their emacs aliases.
    kb.add_binding(KM::NONE, KC::Home, extend(MT::LineEdge(D::Backward)));
    kb.add_binding(KM::NONE, KC::End, extend(MT::LineEdge(D::Forward)));
    kb.add_binding(
        KM::CONTROL,
        KC::Char('a'),
        extend(MT::LineEdge(D::Backward)),
    );
    kb.add_binding(KM::CONTROL, KC::Char('e'), extend(MT::LineEdge(D::Forward)));
    // Buffer edges on Ctrl-Home/End and the Alt-</> jumps.
    kb.add_binding(KM::CONTROL, KC::Home, extend(MT::BufferEdge(D::Backward)));
    kb.add_binding(KM::CONTROL, KC::End, extend(MT::BufferEdge(D::Forward)));
    kb.add_binding(KM::ALT, KC::Char('<'), extend(MT::BufferEdge(D::Backward)));
    kb.add_binding(KM::ALT, KC::Char('>'), extend(MT::BufferEdge(D::Forward)));
    // The kitty keyboard protocol spellings of Alt-</>.
    kb.add_binding(
        KM::SHIFT | KM::ALT,
        KC::Char(','),
        extend(MT::BufferEdge(D::Backward)),
    );
    kb.add_binding(
        KM::SHIFT | KM::ALT,
        KC::Char('.'),
        extend(MT::BufferEdge(D::Forward)),
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
        KC::Up,
        edit_bind(EC::MoveLineUp { select: true }),
    );
    kb.add_binding(
        KM::SHIFT,
        KC::Down,
        edit_bind(EC::MoveLineDown { select: true }),
    );
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
mod test {
    use super::*;

    #[rstest::rstest]
    #[case(KeyModifiers::NONE, KeyCode::Backspace, EditCommand::Backspace)]
    #[case(KeyModifiers::NONE, KeyCode::Delete, EditCommand::Delete)]
    #[case(KeyModifiers::CONTROL, KeyCode::Backspace, EditCommand::BackspaceWord)]
    fn shifted_deletion_keys_work_in_insert_modes(
        #[case] modifier: KeyModifiers,
        #[case] key_code: KeyCode,
        #[case] command: EditCommand,
    ) {
        use crate::{EditMode, Emacs, ReedlineRawEvent, Vi};
        use crossterm::event::{Event, KeyEvent};

        let key = || {
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                key_code,
                modifier | KeyModifiers::SHIFT,
            )))
            .unwrap()
        };
        let expected = edit_bind(command);
        assert_eq!(Emacs::default().parse_event(key()), expected);
        assert_eq!(Vi::default().parse_event(key()), expected);
    }

    #[rstest::rstest]
    #[case(KeyModifiers::NONE, KeyCode::Backspace, true)]
    #[case(KeyModifiers::NONE, KeyCode::Delete, true)]
    #[case(KeyModifiers::CONTROL, KeyCode::Backspace, true)]
    #[case(KeyModifiers::ALT, KeyCode::Enter, true)]
    #[case(KeyModifiers::CONTROL | KeyModifiers::ALT, KeyCode::F(1), true)]
    #[case(KeyModifiers::NONE, KeyCode::Char('x'), false)]
    #[case(KeyModifiers::CONTROL, KeyCode::Char('x'), false)]
    fn shift_fallback_preserves_modifiers_and_explicit_bindings(
        #[case] modifier: KeyModifiers,
        #[case] key_code: KeyCode,
        #[case] falls_back: bool,
    ) {
        let mut kb = Keybindings::new();
        let shifted = modifier | KeyModifiers::SHIFT;
        assert_eq!(kb.find_binding(shifted, key_code), None);

        kb.add_binding(modifier, key_code, ReedlineEvent::Enter);
        assert_eq!(
            kb.find_binding(shifted, key_code),
            falls_back.then_some(ReedlineEvent::Enter)
        );

        kb.add_binding(shifted, key_code, ReedlineEvent::Esc);
        assert_eq!(kb.find_binding(shifted, key_code), Some(ReedlineEvent::Esc));
    }

    #[test]
    fn empty_until_found_is_accepted() {
        let mut kb = Keybindings::new();
        kb.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('x'),
            ReedlineEvent::UntilFound(vec![]),
        );
        assert_eq!(
            kb.find_binding(KeyModifiers::NONE, KeyCode::Char('x')),
            Some(ReedlineEvent::UntilFound(vec![]))
        );
    }

    #[test]
    fn rebinding_a_key_replaces_the_previous_event() {
        let mut kb = Keybindings::new();
        kb.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('r'),
            ReedlineEvent::SearchHistory,
        );
        kb.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('r'),
            ReedlineEvent::ClearScreen,
        );
        assert_eq!(
            kb.find_binding(KeyModifiers::CONTROL, KeyCode::Char('r')),
            Some(ReedlineEvent::ClearScreen)
        );
        assert_eq!(kb.get_keybindings().len(), 1);
    }

    #[test]
    fn remove_binding_returns_the_bound_event_and_unbinds() {
        let mut kb = Keybindings::new();
        kb.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::Menu("m".into()),
        );
        assert_eq!(
            kb.remove_binding(KeyModifiers::NONE, KeyCode::Tab),
            Some(ReedlineEvent::Menu("m".into()))
        );
        assert_eq!(kb.find_binding(KeyModifiers::NONE, KeyCode::Tab), None);
        assert_eq!(kb.remove_binding(KeyModifiers::NONE, KeyCode::Tab), None);
    }

    fn combo(c: char) -> KeyCombination {
        KeyCombination {
            modifier: KeyModifiers::NONE,
            key_code: KeyCode::Char(c),
        }
    }

    #[test]
    fn sequence_binding_is_found_exactly_and_as_a_prefix() {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);

        assert!(kb.is_sequence_prefix(&[combo('j')]));
        assert!(!kb.is_sequence_prefix(&[combo('j'), combo('j')]));
        assert_eq!(
            kb.find_sequence_binding(&[combo('j'), combo('j')]),
            Some(ReedlineEvent::Esc)
        );
        assert_eq!(kb.find_sequence_binding(&[combo('j')]), None);
        assert_eq!(kb.find_sequence_binding(&[combo('j'), combo('x')]), None);
    }

    #[test]
    fn a_key_can_be_a_binding_and_a_prefix_at_once() {
        let mut kb = Keybindings::new();
        kb.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        kb.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);

        // The single-key table is unchanged.
        assert_eq!(
            kb.find_binding(KeyModifiers::NONE, KeyCode::Char('j')),
            Some(ReedlineEvent::Down)
        );
        // The trie still reports `j` as the start of a longer sequence.
        assert!(kb.is_sequence_prefix(&[combo('j')]));
        assert_eq!(
            kb.find_sequence_binding(&[combo('j'), combo('j')]),
            Some(ReedlineEvent::Esc)
        );
    }

    #[test]
    fn one_key_sequence_delegates_to_the_single_key_table() {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[combo('x')], ReedlineEvent::ClearScreen);

        assert_eq!(
            kb.find_binding(KeyModifiers::NONE, KeyCode::Char('x')),
            Some(ReedlineEvent::ClearScreen)
        );
        assert_eq!(kb.get_keybindings().len(), 1);
        assert!(!kb.is_sequence_prefix(&[combo('x')]));
    }

    #[test]
    fn empty_sequence_is_a_no_op() {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[], ReedlineEvent::Esc);
        assert!(kb.get_keybindings().is_empty());
        assert!(!kb.is_sequence_prefix(&[]));
    }

    #[test]
    fn longest_sequence_prefix_prefers_the_longest_complete_sequence() {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[combo('a'), combo('b')], ReedlineEvent::Up);
        kb.add_binding_sequence(&[combo('a'), combo('b'), combo('c')], ReedlineEvent::Down);

        assert_eq!(
            kb.longest_sequence_prefix(&[combo('a'), combo('b'), combo('c')]),
            Some((3, ReedlineEvent::Down))
        );
        assert_eq!(
            kb.longest_sequence_prefix(&[combo('a'), combo('b'), combo('x')]),
            Some((2, ReedlineEvent::Up))
        );
        assert_eq!(kb.longest_sequence_prefix(&[combo('a')]), None);
    }

    #[test]
    fn a_table_without_sequences_deserializes() {
        // Old serialized tables have no `sequences` field.
        let json = r#"{"bindings":{}}"#;
        let kb: Keybindings = serde_json::from_str(json).expect("deserialize");
        assert!(kb.get_keybindings().is_empty());
        assert!(!kb.is_sequence_prefix(&[combo('j')]));
    }
}
