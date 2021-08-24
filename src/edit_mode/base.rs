use crossterm::event::Event;

use crate::{enums::ReedlineEvent, PromptEditMode};

use super::keybindings::Keybindings;

/// Define the style of parsing for the edit events
/// Available default options:
/// - Emacs
/// - Vi
pub trait EditMode {
    /// Translate the given user input event into what the LineEditor understands
    fn parse_event(&mut self, event: Event) -> ReedlineEvent;

    fn update_keybindings(&mut self, keybindings: Keybindings);

    /// What to display in the prompt indicator
    fn edit_mode(&self) -> PromptEditMode;
}
