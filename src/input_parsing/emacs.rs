use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    default_emacs_keybindings,
    enums::{EditCommand, ReedlineEvent},
    PromptEditMode,
};

use super::{keybindings::Keybindings, InputParser};

pub struct EmacsInputParser {
    keybindings: Keybindings,
}

impl Default for EmacsInputParser {
    fn default() -> Self {
        EmacsInputParser {
            keybindings: default_emacs_keybindings(),
        }
    }
}

impl InputParser for EmacsInputParser {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent {
        match event {
            Event::Key(KeyEvent { code, modifiers }) => match (modifiers, code) {
                (KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (KeyModifiers::CONTROL, KeyCode::Char('d')) => ReedlineEvent::CtrlD,
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => ReedlineEvent::CtrlC,
                (KeyModifiers::CONTROL, KeyCode::Char('l')) => ReedlineEvent::ClearScreen,
                (KeyModifiers::NONE, KeyCode::Char(c))
                | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    ReedlineEvent::EditInsert(EditCommand::InsertChar(c))
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => {
                    if let Some(binding) = self.keybindings.find_binding(modifiers, code) {
                        ReedlineEvent::Edit(binding)
                    } else {
                        ReedlineEvent::Edit(vec![])
                    }
                }
            },

            Event::Mouse(_) => ReedlineEvent::Mouse,
            Event::Resize(width, height) => ReedlineEvent::Resize(width, height),
        }
    }

    // HACK: This about this interface more
    fn update_keybindings(&mut self, keybindings: Keybindings) {
        self.keybindings = keybindings;
    }

    fn edit_mode(&self) -> PromptEditMode {
        PromptEditMode::Emacs
    }
}

impl EmacsInputParser {
    pub fn new(keybindings: Keybindings) -> Self {
        EmacsInputParser { keybindings }
    }
}
