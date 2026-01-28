use crate::{
    edit_mode::{
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            add_common_selection_bindings, edit_bind, KeyCombination, KeySequenceState,
            Keybindings,
        },
        EditMode,
    },
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode,
};
use crossterm::event::{KeyCode, KeyModifiers};


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
    fn parse_key_event(&mut self, modifiers: KeyModifiers, code: KeyCode) -> ReedlineEvent {
        let combo = KeyCombination::from((modifiers, code));
        let keybindings = &self.keybindings;
        let resolution = self.sequence_state.process_combo(keybindings, combo);
        resolution
            .into_event(|combo| self.default_key_event(keybindings, combo))
            .unwrap_or(ReedlineEvent::None)
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Emacs
    }

    fn has_pending_sequence(&self) -> bool {
        self.sequence_state.is_pending()
    }

    fn flush_pending_sequence(&mut self) -> Option<ReedlineEvent> {
        let keybindings = &self.keybindings;
        let resolution = self.sequence_state.flush_with_combos();
        resolution.into_event(|combo| self.default_key_event(keybindings, combo))
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
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::enums::ReedlineRawEvent;
    use crossterm::event::{Event, KeyEvent};
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
