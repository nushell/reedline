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
                (KeyModifiers::NONE, KeyCode::Tab) => ReedlineEvent::HandleTab,
                (KeyModifiers::CONTROL, KeyCode::Char('d')) => ReedlineEvent::CtrlD,
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => ReedlineEvent::CtrlC,
                (KeyModifiers::CONTROL, KeyCode::Char('l')) => ReedlineEvent::ClearScreen,
                (KeyModifiers::NONE, KeyCode::Char(c))
                | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])
                }
                (m, KeyCode::Char(c)) if m == KeyModifiers::CONTROL | KeyModifiers::ALT => {
                    ReedlineEvent::EditInsert(EditCommand::InsertChar(c))
                }
                (KeyModifiers::NONE, KeyCode::Enter) => ReedlineEvent::Enter,
                _ => {
                    if let Some(binding) = self.keybindings.find_binding(modifiers, code) {
                        binding
                    } else {
                        ReedlineEvent::Edit(vec![])
                    }
                }
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
