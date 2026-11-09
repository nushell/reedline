mod example;
mod simple_match;

use crate::{auto_pairs::AutoPairContext, StyledText};

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

    /// Returns `true` if the automatic pairing behaviour described by `context`
    /// should happen, `false` to suppress it and run the originally typed
    /// [`EditCommand`](crate::EditCommand) verbatim instead (`InsertChar(open)` for
    /// [`crate::AutoPairAction::Open`], `InsertChar(close)` for
    /// [`crate::AutoPairAction::SkipExistingCloser`], `Backspace` for
    /// [`crate::AutoPairAction::BackspacePair`])
    ///
    /// The default implementation always returns `true` (always auto-pair)
    ///
    /// See [`Reedline::with_auto_pairs`](crate::Reedline::with_auto_pairs) for how
    /// this interacts with pair lookup order and with pasted text.
    fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
        let _ = context;
        true
    }

    /// Tells how a history hint closes the auto-pair closers after the cursor,
    /// so that the hint can be shown and inserted in place of them.
    ///
    /// This is asked only for a [`Hinter`](crate::Hinter) whose
    /// [`continues_text_before_cursor`](crate::Hinter::continues_text_before_cursor)
    /// is `true`, when the text after the cursor is made up of nothing but
    /// closers of the configured [`AutoPairs`](crate::AutoPairs), as the `)` in
    /// `f(gs|)`. `line` is the whole buffer, `cursor` the byte offset of the
    /// cursor in it, `closers` the text after the cursor, and `candidate` the
    /// raw text of the hint ([`Hinter::complete_hint`](crate::Hinter::complete_hint)),
    /// which would follow the cursor.
    ///
    /// Return, for each character of `closers` in order, the byte offset in
    /// `candidate` of the character that closes it, so that accepting the hint
    /// puts the closers in `candidate` and none is lost. A closer inside a
    /// string or a comment in `candidate` does not close anything, which is why
    /// only the language of `line` can tell. The offsets index the raw
    /// `candidate`, never the styled text the hinter shows. They must be
    /// strictly increasing, and the character at each must be the closer it
    /// stands for. Any other result is treated as `None`.
    ///
    /// Return `None` when the hint does not close all of `closers`. No hint is
    /// then shown before them and none can be accepted there.
    ///
    /// A partial acceptance inserts a prefix of `candidate` and keeps the
    /// closers whose offsets lie beyond it after the cursor.
    ///
    /// The default implementation returns `None`.
    fn hint_closer_offsets(
        &self,
        line: &str,
        cursor: usize,
        candidate: &str,
        closers: &str,
    ) -> Option<Vec<usize>> {
        let _ = (line, cursor, candidate, closers);
        None
    }
}
