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
                (KeyModifiers::NONE, KeyCode::Char(c)) => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])
                }
                (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c.to_ascii_uppercase())])
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => self
                    .keybindings
                    .find_binding(modifiers, code)
                    .unwrap_or_else(|| ReedlineEvent::Edit(vec![])),
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
    /// Emacs style input parsing constructer if you want to use custom keybindings
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

    #[ignore = "Unsure what the desired behaviour is"]
    #[test]
    fn overriding_default_keybindings_works() {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('l'),
            ReedlineEvent::HandleTab,
        );

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
        });
        let result = emacs.parse_event(ctrl_l);

        assert_eq!(result, ReedlineEvent::HandleTab);
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

    #[ignore = "Unsure what the desired behaviour is"]
    #[test]
    fn return_IDONT_KNOW_WHAT_when_keybinding_is_not_found() {
        let keybindings = Keybindings::default();

        let mut emacs = Emacs::new(keybindings);
        let ctrl_l = Event::Key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            code: KeyCode::Char('l'),
        });
        let result = emacs.parse_event(ctrl_l);

        // Right now this returns ReedlineEvent::Edit(vec![]), but what should we really return
        assert_eq!(result, ReedlineEvent::HandleTab);
    }
}
