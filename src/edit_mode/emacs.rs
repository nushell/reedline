use crate::{
    edit_mode::{
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            add_common_selection_bindings, edit_bind, KeyCombination, KeySequenceState,
            Keybindings,
        },
        EditMode,
    },
    enums::{EditCommand, ReedlineEvent, ReedlineRawEvent},
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
    sequence_state: KeySequenceState,
}

impl Default for Emacs {
    fn default() -> Self {
        Emacs {
            keybindings: default_emacs_keybindings(),
            sequence_state: KeySequenceState::default(),
        }
    }
}

impl EditMode for Emacs {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        match event.into() {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                let combo = Self::normalize_key_combo(modifiers, code);
                let keybindings = &self.keybindings;
                self.sequence_state
                    .process_combo(keybindings, combo, |combo| {
                        Self::single_key_event(keybindings, combo)
                    })
                    .unwrap_or(ReedlineEvent::None)
            }

            Event::Mouse(_) => self.with_flushed_sequence(ReedlineEvent::Mouse),
            Event::Resize(width, height) => {
                self.with_flushed_sequence(ReedlineEvent::Resize(width, height))
            }
            Event::FocusGained => self.with_flushed_sequence(ReedlineEvent::None),
            Event::FocusLost => self.with_flushed_sequence(ReedlineEvent::None),
            Event::Paste(body) => {
                self.with_flushed_sequence(ReedlineEvent::Edit(vec![EditCommand::InsertString(
                    body.replace("\r\n", "\n").replace('\r', "\n"),
                )]))
            }
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Emacs
    }

    fn has_pending_sequence(&self) -> bool {
        self.sequence_state.is_pending()
    }

    fn flush_pending_sequence(&mut self) -> Option<ReedlineEvent> {
        let keybindings = &self.keybindings;
        self.sequence_state
            .flush(|combo| Self::single_key_event(keybindings, combo))
    }
}

impl Emacs {
    /// Emacs style input parsing constructor if you want to use custom keybindings
    pub const fn new(keybindings: Keybindings) -> Self {
        Emacs {
            keybindings,
            sequence_state: KeySequenceState::new(),
        }
    }

    fn normalize_key_combo(modifier: KeyModifiers, code: KeyCode) -> KeyCombination {
        let key_code = match code {
            KeyCode::Char(c) => {
                let c = match modifier {
                    KeyModifiers::NONE => c,
                    _ => c.to_ascii_lowercase(),
                };
                KeyCode::Char(c)
            }
            other => other,
        };

        KeyCombination { modifier, key_code }
    }

    fn single_key_event(keybindings: &Keybindings, combo: KeyCombination) -> ReedlineEvent {
        match combo.key_code {
            KeyCode::Char(c) => keybindings
                .find_binding(combo.modifier, KeyCode::Char(c))
                .unwrap_or_else(|| {
                    if combo.modifier == KeyModifiers::NONE
                        || combo.modifier == KeyModifiers::SHIFT
                        || combo.modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                        || combo.modifier
                            == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    {
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(
                            if combo.modifier == KeyModifiers::SHIFT {
                                c.to_ascii_uppercase()
                            } else {
                                c
                            },
                        )])
                    } else {
                        ReedlineEvent::None
                    }
                }),
            code => keybindings
                .find_binding(combo.modifier, code)
                .unwrap_or(ReedlineEvent::None),
        }
    }

    fn with_flushed_sequence(&mut self, event: ReedlineEvent) -> ReedlineEvent {
        let Some(flush_event) = self.flush_pending_sequence() else {
            return event;
        };

        if matches!(event, ReedlineEvent::None) {
            return flush_event;
        }

        match flush_event {
            ReedlineEvent::Multiple(mut events) => {
                events.push(event);
                ReedlineEvent::Multiple(events)
            }
            other => ReedlineEvent::Multiple(vec![other, event]),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

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
}
