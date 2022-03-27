/// A span of source code, with positions in bytes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Span {
    /// The starting position of the span, in bytes
    pub start: usize,

    /// The ending position of the span, in bytes
    pub end: usize,
}

impl Span {
    /// Creates a new `Span` from start and end inputs.
    /// The end parameter must be greater than or equal to the start parameter.
    ///
    /// # Panics
    /// If `end < start`
    pub fn new(start: usize, end: usize) -> Span {
        assert!(
            end >= start,
            "Can't create a Span whose end < start, start={}, end={}",
            start,
            end
        );

        Span { start, end }
    }
}

/// A trait that defines how to convert a line and position to a list of potential completions in that position.
pub trait Completer: Send {
    /// the action that will take the line and position and convert it to a vector of completions, which include the
    /// span to replace and the contents of that replacement
    fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion>;
}

/// Suggestion returned by the Completer
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Suggestion {
    /// String replacement that will be introduced to the the buffer
    pub value: String,
    /// Optional description for the replacement
    pub description: Option<String>,
    /// Replacement span in the buffer
    pub span: Span,
}
