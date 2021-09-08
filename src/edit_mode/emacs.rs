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
                (m, KeyCode::Char(c)) if m == KeyModifiers::NONE | KeyModifiers::SHIFT => {
                    ReedlineEvent::Edit(vec![EditCommand::InsertChar(c)])
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
