use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    default_emacs_keybindings,
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode,
};

use super::{keybindings::Keybindings, EditMode};

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
                    if modifier == KeyModifiers::SHIFT {
                        let char = c.to_ascii_uppercase();
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(char)])
                    } else if modifier == KeyModifiers::NONE
                        || modifier == KeyModifiers::CONTROL | KeyModifiers::ALT
                        || modifier
                            == KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
                    {
                        ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])
                    } else {
                        self.keybindings
                            .find_binding(modifier, code)
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
