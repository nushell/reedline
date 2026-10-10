//! Optional policy for deciding how history hints interact with the buffer.
//!
//! A policy may narrow the line passed to a [`crate::Hinter`] and return one
//! edit that drives both the preview and whole acceptance. Reedline validates
//! every range before it is used. The policy owns language-specific decisions;
//! the editor only applies the resulting edit.

use std::ops::Range;

use crate::{AutoPairs, PromptEditMode};

/// Read-only editor state used to plan a history hint.
#[derive(Debug, Clone)]
pub struct HintContext<'a> {
    source: &'a str,
    cursor: usize,
    selection: Option<Range<usize>>,
    at_buffer_end: bool,
    edit_mode: &'a PromptEditMode,
    auto_pairs: Option<&'a AutoPairs>,
}

impl<'a> HintContext<'a> {
    pub(crate) fn new(
        source: &'a str,
        cursor: usize,
        selection: Option<Range<usize>>,
        at_buffer_end: bool,
        edit_mode: &'a PromptEditMode,
        auto_pairs: Option<&'a AutoPairs>,
    ) -> Self {
        Self {
            source,
            cursor,
            selection,
            at_buffer_end,
            edit_mode,
            auto_pairs,
        }
    }

    /// The current, unmodified buffer.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The editor's byte cursor position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// A logical selection, excluding a resting block caret.
    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection.clone()
    }

    /// Whether the cursor is at the logical end of the buffer, including a
    /// block caret resting on the last grapheme.
    pub fn at_buffer_end(&self) -> bool {
        self.at_buffer_end
    }

    /// The current prompt edit mode.
    pub fn edit_mode(&self) -> &PromptEditMode {
        self.edit_mode
    }

    /// Configured automatic pairs, if auto-pairs are enabled.
    pub fn auto_pairs(&self) -> impl Iterator<Item = (char, char)> + '_ {
        self.auto_pairs.into_iter().flat_map(|pairs| pairs.pairs())
    }
}

/// The line slice and byte position to pass to `Hinter::handle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintQuery {
    /// A byte range in `HintContext::source()`.
    pub range: Range<usize>,
    /// The byte position within the selected range.
    pub position: usize,
}

impl HintQuery {
    /// Query the whole buffer using the editor's current byte cursor.
    pub fn whole_buffer(context: &HintContext<'_>) -> Self {
        Self {
            range: 0..context.source.len(),
            position: context.cursor,
        }
    }

    /// Construct a query from a source range and a relative byte position.
    pub fn new(range: Range<usize>, position: usize) -> Self {
        Self { range, position }
    }
}

/// One concrete replacement in the current source buffer.
///
/// For a full hint, the replacement must equal the hinter's raw candidate.
/// When the range starts at the cursor, that range controls both preview
/// masking and whole acceptance. A logical-end append instead uses the empty
/// range at `source.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintEdit {
    /// The byte range to replace in the current source.
    pub range: Range<usize>,
    /// Text to place in that range.
    pub replacement: String,
    /// The resulting absolute byte cursor position.
    pub cursor: usize,
}

impl HintEdit {
    /// Construct an edit. Reedline validates its ranges before applying it.
    pub fn new(range: Range<usize>, replacement: impl Into<String>, cursor: usize) -> Self {
        Self {
            range,
            replacement: replacement.into(),
            cursor,
        }
    }
}

/// Decides how a hint relates to the current buffer.
///
/// Policies are optional. Without one, Reedline keeps the legacy end-of-buffer
/// completion behavior and does not infer that matching trailing characters
/// may be consumed.
pub trait HintPolicy: Send {
    /// Select the source range and cursor position passed to the hinter.
    fn query(&self, context: &HintContext<'_>) -> HintQuery {
        HintQuery::whole_buffer(context)
    }

    /// Plan whole-hint acceptance and its preview. Reedline calls this only
    /// when `candidate` is non-empty. The edit replacement must equal the raw
    /// candidate. If the edit starts at the cursor, its range is also the
    /// source range hidden by the preview; source text after that range stays
    /// visible after the hint. Alternatively, an edit may append at
    /// `source.len()..source.len()` when the cursor is at the logical buffer
    /// end, including a block cursor resting on the last grapheme. Reedline
    /// validates ranges, UTF-8 boundaries, and selection state before showing
    /// or applying the edit. Return `None` when the candidate cannot be
    /// represented safely by one of these edits.
    ///
    /// `candidate` is the raw hint suffix returned by
    /// [`crate::Hinter::complete_hint`], not the fully constructed buffer.
    /// The policy plans edits from that raw text, while Reedline paints the
    /// formatted string returned by [`crate::Hinter::handle`]. Hinter
    /// implementations must keep those outputs semantically aligned;
    /// Reedline does not strip formatting or compare the displayed string to
    /// the raw candidate.
    fn plan(&mut self, context: &HintContext<'_>, candidate: &str) -> Option<HintEdit>;

    /// Plan one partial acceptance. Called only when partial acceptance is
    /// requested, so policies can defer parsing until that key event. The
    /// default returns `None`, which makes partial acceptance unavailable for
    /// this policy. Reedline calls this only for the last validated and
    /// displayed candidate while the source, cursor, selection, and edit mode
    /// still match. If that state is stale, Reedline rejects the event without
    /// re-running the hinter.
    ///
    /// When the validated full edit starts at the cursor, a partial edit must
    /// start there and end no later than the full edit's range. This keeps
    /// partial acceptance from deleting visible source text. For an end-of-
    /// buffer append, a partial edit must also append at the buffer end.
    fn plan_partial(
        &mut self,
        _context: &HintContext<'_>,
        _candidate: &str,
        _next_token: &str,
    ) -> Option<HintEdit> {
        None
    }
}
