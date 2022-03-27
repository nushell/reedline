use super::{
    keybindings::{add_common_keybindings, edit_bind, Keybindings},
    EditMode,
};
use crate::{
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

/// Returns the current default emacs keybindings
pub fn default_emacs_keybindings() -> Keybindings {
    use EditCommand as EC;
    use KeyCode as KC;
    use KeyModifiers as KM;

    let mut kb = Keybindings::new();

    // CTRL
    kb.add_binding(KM::CONTROL, KC::Char('d'), ReedlineEvent::CtrlD);
    kb.add_binding(KM::CONTROL, KC::Char('g'), edit_bind(EC::Redo));
    kb.add_binding(KM::CONTROL, KC::Char('z'), edit_bind(EC::Undo));
    kb.add_binding(KM::CONTROL, KC::Char('a'), edit_bind(EC::MoveToLineStart));
    kb.add_binding(
        KM::CONTROL,
        KC::Char('e'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            edit_bind(EC::MoveToLineEnd),
        ]),
    );
    kb.add_binding(KM::CONTROL, KC::Char('k'), edit_bind(EC::CutToEnd));
    kb.add_binding(KM::CONTROL, KC::Char('u'), edit_bind(EC::CutFromStart));
    kb.add_binding(
        KM::CONTROL,
        KC::Char('y'),
        edit_bind(EC::PasteCutBufferBefore),
    );
    kb.add_binding(KM::CONTROL, KC::Char('h'), edit_bind(EC::Backspace));
    kb.add_binding(KM::CONTROL, KC::Char('w'), edit_bind(EC::CutWordLeft));
    kb.add_binding(KM::CONTROL, KC::Char('t'), edit_bind(EC::SwapGraphemes));

    // ALT
    kb.add_binding(KM::ALT, KC::Left, edit_bind(EC::MoveWordLeft));
    kb.add_binding(KM::ALT, KC::Delete, edit_bind(EC::DeleteWord));
    kb.add_binding(KM::ALT, KC::Backspace, edit_bind(EC::BackspaceWord));
    kb.add_binding(
        KM::ALT,
        KC::Right,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight),
        ]),
    );
    kb.add_binding(KM::ALT, KC::Char('b'), edit_bind(EC::MoveWordLeft));
    kb.add_binding(
        KM::ALT,
        KC::Char('f'),
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintWordComplete,
            edit_bind(EC::MoveWordRight),
        ]),
    );
    kb.add_binding(KM::ALT, KC::Char('d'), edit_bind(EC::CutWordRight));
    kb.add_binding(KM::ALT, KC::Char('u'), edit_bind(EC::UppercaseWord));
    kb.add_binding(KM::ALT, KC::Char('l'), edit_bind(EC::LowercaseWord));
    kb.add_binding(KM::ALT, KC::Char('c'), edit_bind(EC::CapitalizeChar));
    kb.add_binding(
        KM::ALT,
        KC::Char('m'),
        ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
    );

    add_common_keybindings(&mut kb);

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
            Event::Key(KeyEvent { code, modifiers }) => match (modifiers, code) {
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
                            .find_binding(modifier, KeyCode::Char(c))
                            .unwrap_or(ReedlineEvent::None)
                    }
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => self
                    .keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or(ReedlineEvent::None),
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
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
    use pretty_assertions::assert_eq;

    #[test]
    fn ctrl_l_leads_to_clear_screen_event() {
        let mut emacs = Emacs::default();
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
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
        );

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
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
        });
        let result = emacs.parse_event(uppercase_l);

        assert_eq!(
            result,
            ReedlineEvent::Edit(vec![EditCommand::InsertChar('😀')])
        );
    }
}
