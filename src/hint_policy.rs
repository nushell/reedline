//! Optional policy for deciding how history hints interact with the buffer.
//!
//! A policy may narrow the line passed to a [`crate::Hinter`] and describe
//! preview and acceptance edits. Reedline validates every range before it is
//! used. The policy owns language-specific decisions; the editor only applies
//! the resulting edit.

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

/// The independent overlay used while painting a hint.
///
/// Conceptually, the preview text is the source prefix before the cursor,
/// followed by the raw candidate, followed by the retained source suffix
/// after `hidden_range`. That text must match the result of the complete
/// acceptance edit, or Reedline rejects the plan. Painting uses the formatted
/// output from `Hinter::handle` for the candidate portion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintPreview {
    /// Existing source text hidden by the hint. In the initial API this range
    /// must begin at the cursor; any text after its end remains visible after
    /// the hint.
    pub hidden_range: Range<usize>,
    /// Byte position in the source where the hint overlay begins. The initial
    /// renderer supports overlays at the cursor.
    pub overlay_position: usize,
}

impl HintPreview {
    /// Construct preview metadata. Reedline validates its ranges before paint.
    pub fn new(hidden_range: Range<usize>, overlay_position: usize) -> Self {
        Self {
            hidden_range,
            overlay_position,
        }
    }
}

/// A full acceptance edit and its separate preview description.
///
/// A plan with `preview: None` is drawn as a conventional hint appended after
/// the current buffer; Reedline accepts it only as an end-of-buffer append of
/// the raw candidate. A policy should return `None` from `plan` when it cannot
/// safely offer the candidate for display and acceptance. Reedline calls
/// `HintPolicy::plan` only for non-empty raw candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintPlan {
    /// The edit used by whole-hint acceptance.
    pub edit: HintEdit,
    /// The edit-free preview shown by painting.
    pub preview: Option<HintPreview>,
}

impl HintPlan {
    /// Construct a plan with an optional preview.
    pub fn new(edit: HintEdit, preview: Option<HintPreview>) -> Self {
        Self { edit, preview }
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

    /// Plan whole-hint acceptance and, independently, an optional preview.
    /// Reedline calls this only when `candidate` is non-empty and rejects a
    /// preview whose text would differ from the resulting whole-edit buffer.
    /// `candidate` is the raw hint suffix returned by
    /// [`crate::Hinter::complete_hint`], not the fully constructed buffer.
    /// The policy plans edits from that raw text, while Reedline displays the
    /// formatted string returned by [`crate::Hinter::handle`]. Hinter
    /// implementations must keep those two outputs semantically aligned;
    /// Reedline does not strip formatting or compare the displayed string to
    /// the raw candidate.
    fn plan(&mut self, context: &HintContext<'_>, candidate: &str) -> Option<HintPlan>;

    /// Plan one partial acceptance. Called only when partial acceptance is
    /// requested, so policies can defer parsing until that key event. Reedline
    /// calls this only for the last validated and displayed candidate while
    /// the source, cursor, selection, and edit mode still match. If that state
    /// is stale, Reedline rejects the event without re-running the hinter.
    fn plan_partial(
        &mut self,
        context: &HintContext<'_>,
        candidate: &str,
        next_token: &str,
    ) -> Option<HintEdit>;
}
