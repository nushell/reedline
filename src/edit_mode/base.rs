use crate::{
    enums::{EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode,
};

/// Buffer snapshot passed to an [`EditMode`] when resolving a [`MotionTarget`].
pub struct EditContext<'a> {
    pub buffer: &'a str,
    pub cursor: usize,
}

/// The target shape of a motion. Each [`EditMode`] decides what buffer offset
/// the target resolves to using its own segmentation rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionTarget {
    WordLeft,
    WordRight,
    WordRightStart,
    WordRightEnd,
    BigWordLeft,
    BigWordRightStart,
    BigWordRightEnd,
}

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
    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }

    /// Resolve a [`MotionTarget`] to a buffer offset using this mode's segmentation rules.
    /// Returning `None` defers to the default `LineBuffer` behavior for the
    /// equivalent legacy command.
    fn resolve_motion(&self, _target: MotionTarget, _ctx: &EditContext) -> Option<usize> {
        None
    }
}
