use crate::{
    enums::{EventStatus, ReedlineEvent, ReedlineRawEvent},
    PromptEditMode,
};

/// Define the style of parsing for the edit events
/// Available default options:
/// - Emacs
/// - Vi
/// - Helix
pub trait EditMode: Send {
    /// Translate the given user input event into what the `LineEditor` understands
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent;

    /// What to display in the prompt indicator
    fn edit_mode(&self) -> PromptEditMode;

    /// Handles events that apply only to specific edit modes.
    ///
    /// This is also how a [`ReedlineEvent::SwitchMode`] finds its machine. The
    /// engine offers the target to the active mode and then to every standby
    /// registered with `Reedline::with_additional_edit_mode`, and activates the
    /// first one to answer `EventStatus::Handled`. So a mode has to:
    ///
    /// - answer `Handled` for every [`PromptEditMode`] it can report from
    ///   [`edit_mode`](Self::edit_mode), moving into that state and dropping
    ///   any half-typed sequence, or no binding can ever reach it;
    /// - answer `Inapplicable` for everything else *without changing state*,
    ///   since a standby that declines stays a standby and would otherwise be
    ///   left altered by a switch that went elsewhere.
    ///
    /// The engine never asks about the state the active machine already
    /// reports: that switch is declined up front as not a move. A machine
    /// still answers `Handled` for it, since as a standby it may be asked for
    /// the very state it rests in.
    ///
    /// The default declines everything, which suits a mode that is only ever
    /// the active one.
    ///
    /// Only the machine's own state changes here. Any cursor repair the flip
    /// implies is the engine's job, stated over the rest policy the mode maps
    /// to — see `Reedline::change_edit_mode`.
    fn handle_mode_specific_event(&mut self, _event: ReedlineEvent) -> EventStatus {
        EventStatus::Inapplicable
    }
}
