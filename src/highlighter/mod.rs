mod example;
mod simple_match;

use std::ops::Range;

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

/// Which automatic pairing behaviour is about to happen
///
/// Passed to [`Highlighter::should_auto_pair`] via [`AutoPairContext`] so
/// implementations can apply different veto rules per action
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPairAction {
    /// An opening character was typed; the pair would be inserted around the cursor
    /// (or around the current selection, if any)
    Open,
    /// A closing character was typed and the same character already sits at the
    /// cursor; the cursor would move over it instead of inserting
    SkipExistingCloser,
    /// Backspace was pressed between the two halves of an empty pair; both would
    /// be deleted as one step
    BackspacePair,
}

/// Describes an automatic pairing behaviour that reedline is about to perform
///
/// Passed to [`Highlighter::should_auto_pair`] so implementations can inspect the
/// buffer and cursor to decide whether the behaviour should actually happen (e.g.
/// suppressing it inside string literals, or in the middle of a word)
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct AutoPairContext<'a> {
    buffer: &'a str,
    insertion_point: usize,
    pair: (char, char),
    selection: Option<Range<usize>>,
    action: AutoPairAction,
}

impl<'a> AutoPairContext<'a> {
    pub(crate) fn new(
        buffer: &'a str,
        insertion_point: usize,
        pair: (char, char),
        selection: Option<Range<usize>>,
        action: AutoPairAction,
    ) -> Self {
        Self {
            buffer,
            insertion_point,
            pair,
            selection,
            action,
        }
    }

    /// The full contents of the line being edited
    pub fn buffer(&self) -> &'a str {
        self.buffer
    }

    /// The cursor position as a UTF-8 byte offset into [`Self::buffer`]
    pub fn insertion_point(&self) -> usize {
        self.insertion_point
    }

    /// The `(open, close)` pair the pending action would act on
    pub fn pair(&self) -> (char, char) {
        self.pair
    }

    /// The current selection, if any, as a byte range into [`Self::buffer`]
    ///
    /// Always ordered `start <= end`, regardless of which end the selection
    /// anchor sits on relative to the cursor
    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection.clone()
    }

    /// Which automatic behaviour is about to happen
    pub fn action(&self) -> AutoPairAction {
        self.action
    }
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
    /// [`AutoPairAction::Open`], `InsertChar(close)` for
    /// [`AutoPairAction::SkipExistingCloser`], `Backspace` for
    /// [`AutoPairAction::BackspacePair`])
    ///
    /// The default implementation always returns `true` (always auto-pair)
    ///
    /// See [`Reedline::with_auto_pairs`](crate::Reedline::with_auto_pairs) for how
    /// this interacts with pair lookup order and with pasted text.
    fn should_auto_pair(&self, context: &AutoPairContext<'_>) -> bool {
        let _ = context;
        true
    }
}
