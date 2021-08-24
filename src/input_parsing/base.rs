use crossterm::event::Event;

use crate::{enums::ReedlineEvent, PromptEditMode};

use super::keybindings::Keybindings;

pub trait InputParser {
    fn parse_event(&mut self, event: Event) -> ReedlineEvent;
    fn update_keybindings(&mut self, keybindings: Keybindings);
    fn edit_mode(&self) -> PromptEditMode;
}
