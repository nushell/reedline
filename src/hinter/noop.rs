use super::Hinter;

/// Hinter that does not show or complete a hint
///
/// Hacky way of allowing to disable hints
#[derive(Default)]
pub struct NoOpHinter {}

impl Hinter for NoOpHinter {
    fn handle(
        &mut self,
        _line: &str,
        _pos: usize,
        _history: &dyn crate::History,
        _use_ansi_coloring: bool,
    ) -> String {
        String::new()
    }

    fn complete_hint(&self) -> String {
        String::new()
    }

    fn next_hint_token(&self) -> String {
        String::new()
    }
}
