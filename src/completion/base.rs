use nu_ansi_term::Style;
use std::ops::Range;

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
            "Can't create a Span whose end < start, start={start}, end={end}"
        );

        Span { start, end }
    }
}

/// A trait that defines how to convert some text and a position to a list of potential completions in that position.
/// The text could be a part of the whole line, and the position is the index of the end of the text in the original line.
pub trait Completer: Send {
    /// the action that will take the line and position and convert it to a vector of completions, which include the
    /// span to replace and the contents of that replacement
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion>;

    /// same as [`Completer::complete`] but it will return a vector of ranges of the strings
    /// the suggestions are based on
    fn complete_with_base_ranges(
        &mut self,
        line: &str,
        pos: usize,
    ) -> (Vec<Suggestion>, Vec<Range<usize>>) {
        let mut ranges = vec![];
        let suggestions = self.complete(line, pos);
        for suggestion in &suggestions {
            ranges.push(suggestion.span.start..suggestion.span.end);
        }
        ranges.dedup();
        (suggestions, ranges)
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
    ) -> Vec<Suggestion> {
        self.complete(line, pos)
            .into_iter()
            .skip(start)
            .take(offset)
            .collect()
    }

    /// number of available completions
    fn total_completions(&mut self, line: &str, pos: usize) -> usize {
        self.complete(line, pos).len()
    }
}

/// Suggestion returned by the Completer
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// String replacement that will be introduced to the the buffer
    pub value: String,
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
}
