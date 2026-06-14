mod example;
mod simple_match;

use crate::StyledText;

pub use example::ExampleHighlighter;
pub use simple_match::SimpleMatchHighlighter;

/// The context in which abbreviation expansion is being attempted
///
/// Passed to [`Highlighter::should_expand_abbr`] so implementations can apply
/// different veto rules depending on which expansion triggered the check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbbrExpandContext {
    /// Fish-style word abbreviation
    WordAbbreviation,
    /// Bashism history expansion
    #[cfg(feature = "bashisms")]
    BangExpansion,
}

/// The syntax highlighting trait. Implementers of this trait will take in the current string and then
/// return a `StyledText` object, which represents the contents of the original line as styled strings
pub trait Highlighter: Send {
    /// The action that will handle the current buffer as a line and return the corresponding `StyledText` for the buffer
    ///
    /// Cursor position as byte offsets in the string
    fn highlight(&self, line: &str, cursor: usize) -> StyledText;

    /// Returns `true` if an abbreviation should be expanded at the given cursor position
    /// (a byte offset into `line`), `false` if expansion should be suppressed
    ///
    /// `context` indicates which kind of expansion is being attempted so implementations
    /// can apply different veto rules per site
    ///
    /// The default implementation always returns `true` (always expand)
    fn should_expand_abbr(&self, line: &str, cursor: usize, context: AbbrExpandContext) -> bool {
        let _ = (line, cursor, context);
        true
    }
}
