use nu_ansi_term::Style;
use std::ops::Range;
use std::sync::Arc;

/// A span of source code, with positions in bytes
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Span {
    /// The starting position of the span, in bytes
    pub start: usize,

    /// The ending position of the span, in bytes
    pub end: usize,
}

/// A shared, immutable list of completion suggestions.
///
/// Held behind an [`Arc`] so a completer that caches results can hand the same
/// list to reedline on every keystroke, without massive penalty
pub type Suggestions = Arc<[Suggestion]>;

impl Span {
    /// Creates a new `Span` from start and end inputs.
    /// The end parameter must be greater than or equal to the start parameter.
    ///
    /// # Panics
    /// If `end < start`
    pub fn new(start: usize, end: usize) -> Span {
        assert!(
            end >= start,
            "Can't create a Span whose end < start, start={start}, end={end}"
        );

        Span { start, end }
    }
}

/// The outcome of a [`Completer::complete`] request.
///
/// A synchronous completer only ever produces [`Fresh`](Self::Fresh). An
/// asynchronous completer that computes in the background reports its progress
/// through this type.
#[derive(Debug, Clone)]
pub enum CompletionResult {
    /// Final, authoritative results. No further computation is in flight.
    Fresh(Suggestions),
    /// Best-effort results to show in the moment; a fresh computation is still running and
    /// will replace these once it finishes.
    Stale(Suggestions),
    /// No results are available yet; a computation is spinning in the background.
    Pending,
}

impl CompletionResult {
    /// Wrap authoritative results.
    pub fn fresh(suggestions: impl Into<Suggestions>) -> Self {
        CompletionResult::Fresh(suggestions.into())
    }

    /// Best-effort fallback while an authoritative result is still computing:
    /// [`Stale`](Self::Stale) when there is something to show now, else
    /// [`Pending`](Self::Pending).
    pub fn stale_or_pending(fallback: Suggestions) -> Self {
        if fallback.is_empty() {
            CompletionResult::Pending
        } else {
            CompletionResult::Stale(fallback)
        }
    }

    /// Borrow the suggestions this result carries (empty for [`Pending`](Self::Pending)).
    pub fn suggestions(&self) -> &[Suggestion] {
        match self {
            CompletionResult::Fresh(values) | CompletionResult::Stale(values) => values,
            CompletionResult::Pending => &[],
        }
    }

    /// Move the shared suggestion list out of the result without copying.
    ///
    /// Returns `None` for [`Pending`](Self::Pending) nothing is settled yet, so
    /// callers should keep whatever they are already displaying.
    pub fn into_shared(self) -> Option<Suggestions> {
        match self {
            CompletionResult::Fresh(values) | CompletionResult::Stale(values) => Some(values),
            CompletionResult::Pending => None,
        }
    }

    /// Whether there is nothing to show yet because a computation is in flight.
    /// When `true`, callers should preserve any results already displayed.
    pub fn is_pending(&self) -> bool {
        matches!(self, CompletionResult::Pending)
    }
}

/// Vitality of a completer's background work, grabbed by the engine once per
/// event-loop iteration. It tells the engine whether to keep polling for
/// input (rather than blocking) and when a finished result is ready to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionStatus {
    /// No background completion is in flight.... the engine may block on input.
    Idle,
    /// A background completion is still running... the engine should keep polling.
    Pending,
    /// The latest background completion just finished! Its results are now
    /// available and any active menu should be refreshed.
    Ready,
}

/// A trait that defines how to convert some text and a position to a list of potential completions in that position.
/// The text could be a part of the whole line, and the position is the index of the end of the text in the original line.
pub trait Completer {
    /// the action that will take the line and position and convert it to a vector of completions, which include the
    /// span to replace and the contents of that replacement
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult;

    /// same as [`Completer::complete`] but it will also return a vector of ranges
    /// of the strings the suggestions are based on
    fn complete_with_base_ranges(
        &mut self,
        line: &str,
        pos: usize,
    ) -> (CompletionResult, Vec<Range<usize>>) {
        let result = self.complete(line, pos);
        let mut ranges = vec![];
        for suggestion in result.suggestions() {
            ranges.push(suggestion.span.start..suggestion.span.end);
        }
        ranges.dedup();
        (result, ranges)
    }

    /// action that will return a partial section of available completions
    /// this command comes handy when trying to avoid to pull all the data at once
    /// from the completer
    fn partial_complete(
        &mut self,
        line: &str,
        pos: usize,
        start: usize,
        offset: usize,
    ) -> Suggestions {
        self.complete(line, pos)
            .suggestions()
            .iter()
            .skip(start)
            .take(offset)
            .cloned()
            .collect()
    }

    /// number of available completions
    fn total_completions(&mut self, line: &str, pos: usize) -> usize {
        self.complete(line, pos).suggestions().len()
    }

    /// Poll the completer's background work.
    ///
    /// Called once per event-loop iteration by the engine. Synchronous
    /// completers use the default, which always reports
    /// [`CompletionStatus::Idle`].
    fn poll_completion(&mut self) -> CompletionStatus {
        CompletionStatus::Idle
    }
}

/// Suggestion returned by the Completer
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// String replacement that will be introduced to the the buffer
    pub value: String,
    /// If given, overrides `value` as text displayed to user
    pub display_override: Option<String>,
    /// Optional description for the replacement
    pub description: Option<String>,
    /// Optional style for the replacement
    pub style: Option<Style>,
    /// Optional vector of strings in the suggestion. These can be used to
    /// represent examples coming from a suggestion
    pub extra: Option<Vec<String>>,
    /// Replacement span in the buffer
    pub span: Span,
    /// Whether to append a space after selecting this suggestion.
    /// This helps to avoid that a completer repeats the complete suggestion.
    pub append_whitespace: bool,
    /// Indices of the graphemes in the suggestion that matched the typed text.
    /// Useful if using fuzzy matching.
    pub match_indices: Option<Vec<usize>>,
}

impl Suggestion {
    /// Get value to display to user for this suggestion
    pub fn display_value(&self) -> &str {
        self.display_override.as_ref().unwrap_or(&self.value)
    }
}
