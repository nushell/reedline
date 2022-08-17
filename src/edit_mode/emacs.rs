use crate::{
    edit_mode::{
        keybindings::{
            add_common_control_bindings, add_common_edit_bindings, add_common_navigation_bindings,
            edit_bind, Keybindings,
        },
        EditMode,
    },
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = Keybindings::new();
    add_common_control_bindings(&mut kb);
    add_common_navigation_bindings(&mut kb);
    add_common_edit_bindings(&mut kb);

    // *** CTRL ***
    // Moves
    kb.add_binding(
        KM::CONTROL,
        KC::Char('b'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![ReedlineEvent::MenuLeft, ReedlineEvent::Left]),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('f'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::MenuRight,
            ReedlineEvent::Right,
        ]),
    );
    // Undo/Redo
    kb.add_binding(
        KM::CONTROL,
        KC::Char('g'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::Redo),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('z'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::Undo),
    );
    // Cutting
    kb.add_binding(
        KM::CONTROL,
        KC::Char('y'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::PasteCutBufferBefore),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('w'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::CutWordLeft),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('k'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::CutToEnd),
    );
    kb.add_binding(
        KM::CONTROL,
        KC::Char('u'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::CutFromStart),
    );
    // Edits
    kb.add_binding(
        KM::CONTROL,
        KC::Char('t'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::SwapGraphemes),
    );

    // *** ALT ***
    // Moves
    kb.add_binding(
        KM::ALT,
        KC::Left,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveWordLeft),
    );
    kb.add_binding(
        KM::ALT,
        KC::Right,
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight),
        ]),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('b'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::MoveWordLeft),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('f'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight),
        ]),
    );
    // Edits
    kb.add_binding(
        KM::ALT,
        KC::Delete,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::DeleteWord),
    );
    kb.add_binding(
        KM::ALT,
        KC::Backspace,
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::BackspaceWord),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('m'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
    );
    // Cutting
    kb.add_binding(
        KM::ALT,
        KC::Char('d'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::CutWordRight),
    );
    // Case changes
    kb.add_binding(
        KM::ALT,
        KC::Char('u'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::UppercaseWord),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('l'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::LowercaseWord),
    );
    kb.add_binding(
        KM::ALT,
        KC::Char('c'),
        KeyEventKind::Press,
        KeyEventState::NONE,
        edit_bind(EC::CapitalizeChar),
    );

    kb
}

/// This parses the incoming Events like a emacs style-editor
pub struct Emacs {
    keybindings: Keybindings,
}

impl Default for Emacs {
    fn default() -> Self {
        Emacs {
            keybindings: default_emacs_keybindings(),
        }
    }
}

impl EditMode for Emacs {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                state,
            }) => match (modifiers, code) {
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
                        self.keybindings
                            .find_binding(modifier, KeyCode::Char(c), kind, state)
                            .unwrap_or(ReedlineEvent::None)
                    }
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => self
                    .keybindings
                    .find_binding(modifiers, code, kind, state)
                    .unwrap_or(ReedlineEvent::None),
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
            Event::FocusGained => ReedlineEvent::FocusGained,
            Event::FocusLost => ReedlineEvent::FocusLost,
            Event::Paste(s) => ReedlineEvent::Paste(s),
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Emacs
    }
}

impl Emacs {
    /// Emacs style input parsing constructor if you want to use custom keybindings
    pub fn new(keybindings: Keybindings) -> Self {
        Emacs { keybindings }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};
    use pretty_assertions::assert_eq;

    #[test]
    fn ctrl_l_leads_to_clear_screen_event() {
        let mut emacs = Emacs::default();
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
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
            KeyEventKind::Press,
            KeyEventState::NONE,
        );

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::HistoryHintComplete);
    }

    #[test]
    fn inserting_character_works() {
        let mut emacs = Emacs::default();
        let l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::NONE,
            code: KeyCode::Char('l'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = emacs.parse_event(l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('l')])
        );
    }

    #[test]
    fn inserting_capital_character_works() {
        let mut emacs = Emacs::default();

        let uppercase_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::SHIFT,
            code: KeyCode::Char('l'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
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
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::None);
    }

    #[test]
    fn inserting_capital_character_for_non_ascii_remains_as_is() {
        let mut emacs = Emacs::default();

        let uppercase_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::SHIFT,
            code: KeyCode::Char('😀'),
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = emacs.parse_event(uppercase_l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('😀')])
        );
    }
}
