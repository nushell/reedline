use crate::{
    enums::{EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode,
};

/// Define the style of parsing for the edit events
/// Available default options:
/// - Emacs
/// - Vi
pub trait EditMode: Send {
    /// Translate the given user input event into what the `LineEditor` understands
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent;

    /// What to display in the prompt indicator
    fn edit_mode(&self) -> PromptEditMode;

    /// Handles events that apply only to specific edit modes (e.g changing vi mode)
    ///
    /// Only the machine's own state changes here. Any cursor repair the flip
    /// implies is the engine's job, stated over the rest policy the mode maps
    /// to — see `Reedline::change_edit_mode`.
    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }
}
