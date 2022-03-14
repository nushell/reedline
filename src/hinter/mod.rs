mod default;
pub use default::DefaultHinter;

use crate::History;
/// A trait that's responsible for returning the hint for the current line and position
/// Hints are often shown in-line as part of the buffer, showing the user text they can accept or ignore
pub trait Hinter: Send {
    /// Handle the hinting duty by using the line, position, and current history
    ///
    /// Returns the formatted output to show the user
    fn handle(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        use_ansi_coloring: bool,
    ) -> String;

    /// Return the current hint unformatted to perform the completion of the full hint
    fn complete_hint(&self) -> String;

    /// Return the first semantic token of the hint
    /// for incremental completion
    fn next_hint_token(&self) -> String;
}
