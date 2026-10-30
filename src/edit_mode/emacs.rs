use crate::{
    edit_mode::{
        combine_events,
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            add_common_selection_bindings, edit_bind, KeyCombination, Keybindings,
        },
        normalized_key_combination, parse_non_key_event, EditMode, SequenceResolution,
        SequenceResolver,
    },
    enums::{EditCommand, EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = Keybindings::new();
    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);
    add_common_selection_bindings(&mut kb);

    // This could be in common, but in Vi it also changes the mode
    kb.add_binding(KM::NONE, KC::Enter, ReedlineEvent::Enter);

    // *** CTRL ***
    // Moves
    kb.add_binding(
        KM::CONTROL,
        KC::Char('b'),
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuLeft, ReedlineEvent::Left]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('f'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::MenuRight,
            ReedlineEvent::Right,
        ]),
    );
    // Undo/Redo
    kb.add_binding(KM::CONTROL, KC::Char('g'), edit_bind(EC::Redo));
    kb.add_binding(KM::CONTROL, KC::Char('z'), edit_bind(EC::Undo));
    // Cutting
    kb.add_binding(
        KM::CONTROL,
        KC::Char('y'),
        edit_bind(EC::PasteCutBufferBefore),
    );
    kb.add_binding(KM::CONTROL, KC::Char('w'), edit_bind(EC::CutWordLeft));
    kb.add_binding(KM::CONTROL, KC::Char('k'), edit_bind(EC::KillLine));
    kb.add_binding(KM::CONTROL, KC::Char('u'), edit_bind(EC::CutFromStart));
    kb.add_binding(KM::ALT, KC::Char('d'), edit_bind(EC::CutWordRight));
    // Edits
    kb.add_binding(KM::CONTROL, KC::Char('t'), edit_bind(EC::SwapGraphemes));

    // *** ALT ***
    // Moves
    kb.add_binding(
        KM::ALT,
        KC::Left,
        edit_bind(EC::MoveWordLeft { select: false }),
    );
    kb.add_binding(
        KM::ALT,
        KC::Right,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight { select: false }),
        ]),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('b'),
        edit_bind(EC::MoveWordLeft { select: false }),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('f'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight { select: false }),
        ]),
    );
    // Edits
    kb.add_binding(KM::ALT, KC::Delete, edit_bind(EC::DeleteWord));
    kb.add_binding(KM::ALT, KC::Backspace, edit_bind(EC::BackspaceWord));
    kb.add_binding(
        KM::ALT,
        KC::Char('m'),
        ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
    );
    // Case changes
    kb.add_binding(KM::ALT, KC::Char('u'), edit_bind(EC::UppercaseWord));
    kb.add_binding(KM::ALT, KC::Char('l'), edit_bind(EC::LowercaseWord));
    kb.add_binding(KM::ALT, KC::Char('c'), edit_bind(EC::CapitalizeChar));

    kb
}

/// This parses the incoming Events like a emacs style-editor
pub struct Emacs {
    keybindings: Keybindings,
    resolver: SequenceResolver,
}

impl Default for Emacs {
    fn default() -> Self {
        Emacs {
            keybindings: default_emacs_keybindings(),
            resolver: SequenceResolver::new(),
        }
    }
}

impl EditMode for Emacs {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => self.dispatch_key(modifiers, code),

            event => parse_non_key_event(event),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Emacs
    }

    fn handle_mode_specific_event(&mut self, event: ReedlineEvent) -> EventStatus {
        // One state, so the only switch emacs accepts is into itself.
        match event {
            ReedlineEvent::SwitchMode(PromptEditMode::Emacs) => {
                self.resolver.reset();
                EventStatus::Handled
            }
            _ => EventStatus::Inapplicable,
        }
    }

    fn has_pending_sequence(&self) -> bool {
        self.resolver.is_pending()
    }

    fn flush_pending_sequence(&mut self) -> ReedlineEvent {
        let pending = self.resolver.take_pending();
        if pending.is_empty() {
            return ReedlineEvent::None;
        }
        combine_events(self.flush_keys(&pending))
    }
}

impl Emacs {
    /// Emacs style input parsing constructor if you want to use custom keybindings
    pub fn new(keybindings: Keybindings) -> Self {
        Emacs {
            keybindings,
            resolver: SequenceResolver::new(),
        }
    }

    /// Route one keypress through the sequence trie first, then the ordinary
    /// single-key table. An unbound plain character still self-inserts.
    fn dispatch_key(&mut self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        let key = normalized_key_combination(modifiers, code);
        match self.resolver.resolve(&self.keybindings, key.clone()) {
            SequenceResolution::Pending => ReedlineEvent::None,
            SequenceResolution::Complete(event) => event,
            SequenceResolution::NoMatch { flush } => {
                if flush.is_empty() {
                    self.single_key_event(modifiers, code)
                } else {
                    let mut events = self.flush_keys(&flush);
                    let current = self.dispatch_key(modifiers, code);
                    if current != ReedlineEvent::None {
                        events.push(current);
                    }
                    combine_events(events)
                }
            }
        }
    }

    /// Resolve keys that were pending when a sequence was interrupted or timed
    /// out. A complete sequence inside the prefix wins; the rest is replayed
    /// through the single-key fallback, so `j` in emacs still inserts `j`.
    fn flush_keys(&self, keys: &[KeyCombination]) -> Vec<ReedlineEvent> {
        let mut events = Vec::new();
        let mut rest = keys;
        while let Some((len, event)) = self.keybindings.longest_sequence_prefix(rest) {
            events.push(event);
            rest = &rest[len..];
        }
        for key in rest {
            let event = self.single_key_event(key.modifier, key.key_code);
            if event != ReedlineEvent::None {
                events.push(event);
            }
        }
        events
    }

    /// The pre-sequence behaviour for one key: the table, then self-insert for
    /// a typeable character.
    fn single_key_event(&self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        match (modifiers, code) {
            (modifier, KeyCode::Char(c)) => {
                // Note. The modifier can also be a combination of modifiers, for
                // example:
                //     KeyModifiers::CONTROL | KeyModifiers::ALT
                //     KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                //
                // Mixed modifiers are used by non american keyboards that have extra
                // keys like 'alt gr'. Keep this in mind if in the future there are
                // cases where an event is not being captured
                let c = match modifier {
                    KeyModifiers::NONE => c,
                    _ => c.to_ascii_lowercase(),
                };

                self.keybindings
                    .find_binding(modifier, KeyCode::Char(c))
                    .unwrap_or_else(|| {
                        if modifier == KeyModifiers::NONE
                            || modifier == KeyModifiers::SHIFT
                            || modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                            || modifier
                                == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                        {
                            ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                                if modifier == KeyModifiers::SHIFT {
                                    c.to_ascii_uppercase()
                                } else {
                                    c
                                },
                            )])
                        } else {
                            ReedlineEvent::None
                        }
                    })
            }
            _ => self
                .keybindings
                .find_binding(modifiers, code)
                .unwrap_or(ReedlineEvent::None),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> ReedlineRawEvent {
        ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(code, modifiers))).unwrap()
    }

    fn combo(c: char) -> KeyCombination {
        KeyCombination {
            modifier: KeyModifiers::NONE,
            key_code: KeyCode::Char(c),
        }
    }

    fn emacs_with_sequence(keys: &[KeyCombination], event: ReedlineEvent) -> Emacs {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding_sequence(keys, event);
        Emacs::new(keybindings)
    }

    #[test]
    fn switch_mode_event_accepts_only_emacs() {
        let mut emacs = Emacs::default();
        assert!(matches!(
            emacs.handle_mode_specific_event(ReedlineEvent::SwitchMode(PromptEditMode::Emacs)),
            EventStatus::Handled
        ));
        assert!(matches!(
            emacs.handle_mode_specific_event(ReedlineEvent::SwitchMode(PromptEditMode::Vi(
                crate::PromptViMode::Normal
            ))),
            EventStatus::Inapplicable
        ));
    }

    #[test]
    fn ctrl_l_leads_to_clear_screen_event() {
        let mut emacs = Emacs::default();
        let ctrl_l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::ClearScreen);
    }

    #[test]
    fn overriding_default_keybindings_works() {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('l'),
            ReedlineEvent::HistoryHintComplete,
        );

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::HistoryHintComplete);
    }

    #[test]
    fn inserting_character_works() {
        let mut emacs = Emacs::default();
        let l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        let result = emacs.parse_event(l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('l')])
        );
    }

    #[test]
    fn inserting_capital_character_works() {
        let mut emacs = Emacs::default();

        let uppercase_l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = emacs.parse_event(uppercase_l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('L')])
        );
    }

    #[test]
    fn return_none_reedline_event_when_keybinding_is_not_found() {
        let keybindings = Keybindings::default();

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn inserting_capital_character_for_non_ascii_remains_as_is() {
        let mut emacs = Emacs::default();

        let uppercase_l = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('😀'),
            KeyModifiers::SHIFT,
        )))
        .unwrap();
        let result = emacs.parse_event(uppercase_l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('😀')])
        );
    }

    #[test]
    fn kill_line() {
        let mut emacs = Emacs::default();

        let ctrl_k = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL,
        )))
        .unwrap();
        let result = emacs.parse_event(ctrl_k);

        assert_eq!(result, ReedlineEvent::Edit(vec![EditCommand::KillLine]));
    }

    // --- multi-key sequences ---

    #[test]
    fn a_complete_sequence_fires_its_event() {
        let mut emacs = emacs_with_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);

        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert!(emacs.has_pending_sequence());
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::Esc
        );
        assert!(!emacs.has_pending_sequence());
    }

    #[test]
    fn a_strict_prefix_waits_for_more_keys() {
        let mut emacs = emacs_with_sequence(
            &[combo('a'), combo('b'), combo('c')],
            ReedlineEvent::ClearScreen,
        );

        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('a'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('b'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert!(emacs.has_pending_sequence());
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('c'), KeyModifiers::NONE)),
            ReedlineEvent::ClearScreen
        );
    }

    #[test]
    fn a_timeout_flushes_the_prefix_through_self_insert() {
        let mut emacs = emacs_with_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);

        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            emacs.flush_pending_sequence(),
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')])
        );
        assert!(!emacs.has_pending_sequence());
    }

    #[test]
    fn an_interrupted_prefix_flushes_then_handles_the_new_key() {
        let mut emacs = emacs_with_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);

        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('j')]),
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')]),
            ])
        );
        assert!(!emacs.has_pending_sequence());
    }

    #[test]
    fn a_key_that_is_both_binding_and_prefix_waits() {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        keybindings.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut emacs = Emacs::new(keybindings);

        // `j` alone is bound, but `jj` continues the sequence, so it waits.
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::Esc
        );

        // On its own, the shorter binding is the timeout fallback.
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(emacs.flush_pending_sequence(), ReedlineEvent::Down);
    }

    #[test]
    fn a_flushed_binding_then_the_new_key_keep_their_order() {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        keybindings.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut emacs = Emacs::new(keybindings);

        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            ReedlineEvent::None
        );
        assert_eq!(
            emacs.parse_event(key(KeyCode::Char('x'), KeyModifiers::NONE)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Down,
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('x')]),
            ])
        );
    }
}
