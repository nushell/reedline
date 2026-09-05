use std::ops::Range;

/// Configuration for automatic pair insertion.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AutoPairs {
    pairs: Vec<(char, char)>,
}

impl AutoPairs {
    /// Create automatic pair insertion configuration from `(open, close)` pairs.
    pub fn new<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (char, char)>,
    {
        Self {
            pairs: pairs.into_iter().collect(),
        }
    }

    pub(crate) fn opening_pair(&self, ch: char) -> Option<(char, char)> {
        self.pairs.iter().find(|pair| pair.0 == ch).copied()
    }

    pub(crate) fn closing_pair(&self, ch: char) -> Option<(char, char)> {
        self.pairs.iter().find(|pair| pair.1 == ch).copied()
    }

    pub(crate) fn pairs(&self) -> impl Iterator<Item = (char, char)> + '_ {
        self.pairs.iter().copied()
    }
}

/// Which automatic pairing behaviour is about to happen.
///
/// Passed to [`crate::Highlighter::should_auto_pair`] via [`AutoPairContext`] so
/// implementations can apply different veto rules per action.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPairAction {
    /// An opening character was typed; the pair would be inserted around the cursor
    /// (or around the current selection, if any).
    Open,
    /// A closing character was typed and the same character already sits at the
    /// cursor; the cursor would move over it instead of inserting.
    SkipExistingCloser,
    /// Backspace was pressed between the two halves of an empty pair; both would
    /// be deleted as one step.
    BackspacePair,
}

/// Describes an automatic pairing behaviour that reedline is about to perform.
///
/// Passed to [`crate::Highlighter::should_auto_pair`] so implementations can inspect the
/// buffer and cursor to decide whether the behaviour should actually happen (e.g.
/// suppressing it inside string literals, or in the middle of a word).
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

    /// The full contents of the line being edited.
    pub fn buffer(&self) -> &'a str {
        self.buffer
    }

    /// The cursor position as a UTF-8 byte offset into [`Self::buffer`].
    pub fn insertion_point(&self) -> usize {
        self.insertion_point
    }

    /// The `(open, close)` pair the pending action would act on.
    pub fn pair(&self) -> (char, char) {
        self.pair
    }

    /// The current selection, if any, as a byte range into [`Self::buffer`].
    ///
    /// Always ordered `start <= end`, regardless of which end the selection
    /// anchor sits on relative to the cursor.
    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection.clone()
    }

    /// Which automatic behaviour is about to happen.
    pub fn action(&self) -> AutoPairAction {
        self.action
    }
}
