//! Collection of common functions that can be used to create menus
use std::borrow::Cow;
use std::ops::Range;
use unicase::UniCase;

use nu_ansi_term::{ansi::RESET, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    menu::{InputMode, MenuSettings, OutputMode},
    painting::Painter,
    CompletionOrigin, CompletionResult, Editor, Partial, Suggestion, Suggestions, UndoBehavior,
};

/// Appended to whatever `truncate_with_ansi` keeps, so callers must allow at
/// least its width.
const TRUNCATION_SUFFIX: &str = "...";

/// Index result obtained from parsing a string with an index marker
/// For example, the next string:
///     "this is an example :10"
///
/// Contains an index marker :10. This marker indicates that the user
/// may want to select the 10th element from a list
#[derive(Debug, PartialEq, Eq)]
pub struct ParseResult<'buffer> {
    /// Text before the marker
    pub remainder: &'buffer str,
    /// Parsed value from the marker
    pub index: Option<usize>,
    /// Marker representation as string
    pub marker: Option<&'buffer str>,
    /// Direction of the search based on the marker
    pub action: ParseAction,
    /// Prefix to search for
    pub prefix: Option<&'buffer str>,
}

/// Direction of the index found in the string
#[derive(Debug, PartialEq, Eq)]
pub enum ParseAction {
    /// Forward index search
    ForwardSearch,
    /// Backward index search
    BackwardSearch,
    /// Last token
    LastToken,
    /// Last executed command.
    LastCommand,
    /// Backward search for a prefix
    BackwardPrefixSearch,
}

/// Splits a string that contains a marker character
///
/// ## Example usage
/// ```
/// use reedline::menu_functions::{parse_selection_char, ParseAction, ParseResult};
///
/// let parsed = parse_selection_char("this is an example!10", '!');
///
/// assert_eq!(
///     parsed,
///     ParseResult {
///         remainder: "this is an example",
///         index: Some(10),
///         marker: Some("!10"),
///         action: ParseAction::ForwardSearch,
///         prefix: None,
///     }
/// )
///
/// ```
pub fn parse_selection_char(buffer: &str, marker: char) -> ParseResult<'_> {
    if buffer.is_empty() {
        return ParseResult {
            remainder: buffer,
            index: None,
            marker: None,
            action: ParseAction::ForwardSearch,
            prefix: None,
        };
    }

    let mut input = buffer.chars().peekable();

    let mut index = 0;
    while let Some(char) = input.next() {
        if char == marker {
            match input.peek() {
                #[cfg(feature = "bashisms")]
                Some(&x) if x == marker => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + 2 * marker.len_utf8()]),
                        action: ParseAction::LastCommand,
                        prefix: None,
                    };
                }
                #[cfg(feature = "bashisms")]
                Some(&x) if x == '$' => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + 2]),
                        action: ParseAction::LastToken,
                        prefix: None,
                    };
                }
                Some(&x) if x.is_ascii_digit() || x == '-' => {
                    let mut count: usize = 0;
                    let mut size: usize = marker.len_utf8();
                    let action = if x == '-' {
                        size += 1;
                        let _ = input.next();
                        ParseAction::BackwardSearch
                    } else {
                        ParseAction::ForwardSearch
                    };
                    while let Some(&c) = input.peek() {
                        if let Some(c) = c.to_digit(10) {
                            let _ = input.next();
                            count *= 10;
                            count += c as usize;
                            size += 1;
                        } else {
                            return ParseResult {
                                remainder: &buffer[0..index],
                                index: Some(count),
                                marker: Some(&buffer[index..index + size]),
                                action,
                                prefix: None,
                            };
                        }
                    }
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(count),
                        marker: Some(&buffer[index..index + size]),
                        action,
                        prefix: None,
                    };
                }
                #[cfg(feature = "bashisms")]
                Some(&x) if x.is_ascii_alphabetic() => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + marker.len_utf8()]),
                        action: ParseAction::BackwardPrefixSearch,
                        prefix: Some(&buffer[index + marker.len_utf8()..buffer.len()]),
                    };
                }
                None => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..buffer.len()]),
                        action: ParseAction::ForwardSearch,
                        prefix: Some(&buffer[index..buffer.len()]),
                    };
                }
                _ => {}
            }
        }
        index += char.len_utf8();
    }

    ParseResult {
        remainder: buffer,
        index: None,
        marker: None,
        action: ParseAction::ForwardSearch,
        prefix: None,
    }
}

/// Find differing substring between two strings.
///
/// Skips the common prefix; if the rest of `old_string` is a suffix of what
/// remains, the difference is the text in between, otherwise everything after
/// the prefix. Returns the byte offset of the difference and the difference.
pub fn string_difference<'a>(new_string: &'a str, old_string: &str) -> (usize, &'a str) {
    let prefix = new_string
        .char_indices()
        .zip(old_string.chars())
        .take_while(|((_, n), o)| n == o)
        .map(|((i, c), _)| i + c.len_utf8())
        .last()
        .unwrap_or(0);

    // `prefix` is a char boundary of both strings by construction, so the
    // fallbacks are unreachable; they only keep the slicing panic-free.
    let (_, old_rest) = old_string.split_at_checked(prefix).unwrap_or_default();
    let (head, new_rest) = new_string.split_at_checked(prefix).unwrap_or_default();
    let diff = new_rest.strip_suffix(old_rest).unwrap_or(new_rest);

    (head.len(), diff)
}

/// Get the part of the line that should be given as input to the completer, as well
/// as the index of the end of that piece of text
///
/// `prev_input` is the text in the buffer when the menu was activated. Needed for `InputMode::Diff`.
pub fn completer_input(
    buffer: &str,
    insertion_point: usize,
    prev_input: Option<&str>,
    input_mode: InputMode,
) -> (String, usize) {
    match input_mode {
        InputMode::FullBuffer => (buffer.to_owned(), insertion_point),
        InputMode::CursorPrefix => {
            // TODO previously, all but the list menu replaced newlines with spaces here
            // The completers should be adapted to account for this, and tests need to be added
            (buffer[..insertion_point].to_owned(), insertion_point)
        }
        InputMode::Diff => {
            if let Some(old_string) = prev_input {
                let (start, input) = string_difference(buffer, old_string);
                if !input.is_empty() {
                    (input.to_owned(), start + input.len())
                } else {
                    (String::new(), insertion_point)
                }
            } else {
                (String::new(), insertion_point)
            }
        }
    }
}

/// Stashes the buffer on first call when in `InputMode::Diff` (so later calls can diff
/// against the original), then resolves the completer input via [`completer_input`].
///
/// Centralises the input-resolution boilerplate shared by all menu `update_values` impls.
pub fn resolve_completer_input(
    editor: &Editor,
    saved_input: &mut Option<String>,
    settings: &MenuSettings,
) -> (String, usize) {
    let mode = settings.effective_input_mode();
    if mode == InputMode::Diff && saved_input.is_none() {
        *saved_input = Some(editor.get_buffer().to_string());
    }
    completer_input(
        editor.get_buffer(),
        editor.completion_point(),
        saved_input.as_deref(),
        mode,
    )
}

/// Find the closest index less than or equal to the current index that's a
/// character boundary
///
/// This is already a method on `str`, but it's nightly-only. Once that becomes
/// stable, this function will be removed.
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        s.len()
    } else {
        (1..=index)
            .rev()
            .find(|i| s.is_char_boundary(*i))
            .unwrap_or(0)
    }
}

/// Number of lines available for the menu body
pub(crate) fn available_lines(painter: &Painter, min_rows: u16, max_lines: u16) -> u16 {
    let lines = painter.remaining_lines_real().min(max_lines);
    if lines == 0 {
        // Handle the case where a prompt uses the entire screen.
        // Drawing the menu has priority over the drawing the prompt.
        painter.remaining_lines().min(min_rows)
    } else {
        lines
    }
}

/// Scroll a fixed window so the selected row stays inside
pub(crate) fn scroll_offset(selected: u16, current: u16, window: u16) -> u16 {
    if selected <= current {
        // Selection is above the visible area, scroll up
        selected
    } else if selected >= current.saturating_add(window) {
        // Selection is below the visible area, scroll down
        selected.saturating_sub(window) + 1
    } else {
        // Selection is within the visible area
        current
    }
}

/// Where a menu stands between its activation and a final answer about the
/// line on screen.
///
/// One value instead of three flags (`awaiting_results`, `provisional_results`,
/// `opening`), whose invariants — pending implies provisional, and only a final
/// answer may end the opening phase — lived in the update order rather than in
/// a type that cannot express their violation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CompletionPhase {
    /// Activated with nothing asked about the line yet: no answer is
    /// outstanding and none is provisional.
    #[default]
    Unasked,
    /// Only provisional answers so far. The menu stays off screen, so one
    /// about to be closed by a lone suggestion never appears.
    Opening(AnswerKind),
    /// A final answer landed once. Later provisional answers keep the menu on
    /// screen: stale values beat blanking it on every keystroke.
    Open(AnswerKind),
}

/// How final the last answer was, as [`CompletionPhase`] remembers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnswerKind {
    /// Computed for the line on screen.
    Fresh,
    /// Cached values for another line: shown, never decided on.
    Stale,
    /// Nothing yet; the background work is still running.
    Pending,
}

impl CompletionPhase {
    /// A new activation forgets the previous line's answer entirely.
    pub(crate) fn on_activate(&mut self) {
        *self = CompletionPhase::Unasked;
    }

    /// Fold the answer to an update into the phase.
    pub(crate) fn note(&mut self, result: &CompletionResult) {
        let kind = if result.is_pending() {
            AnswerKind::Pending
        } else if result.is_provisional() {
            AnswerKind::Stale
        } else {
            AnswerKind::Fresh
        };
        *self = match (*self, kind) {
            // Once open, no later answer can put the menu back off screen.
            (CompletionPhase::Open(_), kind) => CompletionPhase::Open(kind),
            // Only a final answer ends the opening phase.
            (_, AnswerKind::Fresh) => CompletionPhase::Open(AnswerKind::Fresh),
            (_, kind) => CompletionPhase::Opening(kind),
        };
    }

    /// Whether the values on display may still be superseded.
    pub(crate) fn provisional(self) -> bool {
        match self {
            CompletionPhase::Unasked => false,
            CompletionPhase::Opening(kind) | CompletionPhase::Open(kind) => {
                kind != AnswerKind::Fresh
            }
        }
    }

    /// Whether a background answer is still outstanding, so an empty menu
    /// means "still computing" rather than "no records".
    pub(crate) fn awaiting_results(self) -> bool {
        matches!(
            self,
            CompletionPhase::Opening(AnswerKind::Pending)
                | CompletionPhase::Open(AnswerKind::Pending)
        )
    }

    /// Whether no final answer about the line on screen has landed yet.
    pub(crate) fn awaiting_first_answer(self) -> bool {
        !matches!(self, CompletionPhase::Open(_))
    }
}

/// A menu's suggestions, tied to the buffer they were computed against.
/// Spans are validated before reaching the buffer.
#[derive(Default)]
pub struct CompletionDisplay {
    /// Menu suggestions (private; spans via accept/common_prefix)
    values: Suggestions,
    /// Display widths
    pub display_widths: Vec<usize>,
    /// Shortest of the strings the suggestions are based on.
    pub shortest_base_string: String,
    /// Width of the longest suggestion in `values`.
    pub longest_suggestion: usize,
    /// Completer-supplied partial (None = use derivation)
    partial: Option<Partial>,
    /// Buffer/cursor for span validation
    computed_for: CompletionOrigin,
}

impl CompletionDisplay {
    /// Build the display for a completion `result`, or `None` when there is
    /// nothing to adopt yet.
    pub fn from_result(
        result: CompletionResult,
        base_ranges: &[Range<usize>],
        editor: &Editor,
    ) -> Option<Self> {
        match result {
            CompletionResult::Pending => None,
            CompletionResult::Fresh {
                suggestions,
                partial,
            } => {
                // Stamped from live editor (Fresh result)
                let origin = CompletionOrigin::new(editor.get_buffer(), editor.insertion_point());
                Some(Self::new(suggestions, partial, base_ranges, origin))
            }
            CompletionResult::Stale {
                suggestions,
                origin,
                partial,
            } => Some(Self::new(suggestions, partial, base_ranges, origin)),
        }
    }

    /// Adopt suggestions and measure display metrics
    fn new(
        values: Suggestions,
        partial: Option<Partial>,
        base_ranges: &[Range<usize>],
        computed_for: CompletionOrigin,
    ) -> Self {
        let display_widths: Vec<usize> = values
            .iter()
            .map(|suggestion| strip_ansi_escapes::strip_str(suggestion.display_value()).width())
            .collect();

        // Find the maximum width
        let longest_suggestion = display_widths.iter().copied().max().unwrap_or(0);

        // Shortest slice from stored buffer (may differ from editor if stale)
        let buffer = computed_for.buffer.as_str();
        let shortest_base_string = base_ranges
            .iter()
            .map(|range| {
                let end_index = floor_char_boundary(buffer, range.end);
                let start_index = floor_char_boundary(buffer, range.start).min(end_index);
                &buffer[start_index..end_index]
            })
            .min_by_key(|buffer_slice| buffer_slice.width())
            .map(String::from)
            .unwrap_or_default();

        Self {
            values,
            display_widths,
            shortest_base_string,
            longest_suggestion,
            partial,
            computed_for,
        }
    }

    /// Read-only: values, descriptions, widths (no spans)
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.values
    }

    /// Whether spans match the current editor buffer
    fn is_current(&self, editor: &Editor) -> bool {
        self.computed_for
            .matches(editor.get_buffer(), editor.insertion_point())
    }

    /// Accept suggestion at index (no-op if stale or out of range)
    pub fn accept(&self, index: usize, editor: &mut Editor, output_mode: Option<OutputMode>) {
        if self.is_current(editor) {
            replace_in_buffer(self.values.get(index).cloned(), editor, output_mode);
        }
    }

    /// Apply partial completion (completer-supplied or derived). No-op and returns false when stale or unchanged.
    pub fn common_prefix(&self, editor: &mut Editor) -> bool {
        if !self.is_current(editor) {
            return false;
        }
        match &self.partial {
            Some(partial) => apply_partial(partial, editor),
            None => match derive_common_prefix(editor.get_buffer(), &self.values) {
                Some(partial) => apply_partial(&partial, editor),
                None => false,
            },
        }
    }
}

/// Longest common prefix across suggestions sharing a span
fn derive_common_prefix(line: &str, suggestions: &[Suggestion]) -> Option<Partial> {
    let span = suggestions.first()?.span;
    let mut values = suggestions
        .iter()
        .filter(|s| s.span == span)
        .map(|s| s.value.as_str());

    let mut insert = values.next()?.to_string();
    for value in values {
        let shared = insert
            .char_indices()
            .zip(value.chars())
            .find_map(|((i, a), b)| (a != b).then_some(i))
            .unwrap_or(insert.len().min(value.len()));
        insert.truncate(shared);
    }

    let end = floor_char_boundary(line, span.end);
    let start = floor_char_boundary(line, span.start).min(end);
    let entered = line.get(start..end)?;

    // Ensure prefix extends (not overwrites) user input
    let extends = !insert.is_empty()
        && insert != entered
        && UniCase::new(insert.as_str())
            .to_folded_case()
            .contains(&UniCase::new(entered).to_folded_case());
    extends.then_some(Partial { span, insert })
}

/// Apply partial to buffer. No-op if buffer already matches
fn apply_partial(partial: &Partial, editor: &mut Editor) -> bool {
    let buffer = editor.get_buffer();
    let end = floor_char_boundary(buffer, partial.span.end);
    let start = floor_char_boundary(buffer, partial.span.start).min(end);
    if buffer[start..end] == partial.insert {
        return false;
    }
    commit_buffer_replacement(editor, start, end, &partial.insert);
    true
}

/// Apply string replacement to line buffer
fn commit_buffer_replacement(editor: &mut Editor, start: usize, end: usize, replacement: &str) {
    // Use cursor head, clear selection
    let mut line_buffer = std::mem::take(editor.line_buffer_mut());
    let head = line_buffer.cursor().head();

    line_buffer.replace_range(start..end, replacement);
    line_buffer.clear_selection();
    line_buffer.set_insertion_point(
        head.saturating_add(replacement.len())
            .saturating_sub(end - start),
    );

    editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
}

/// Helper to accept a completion suggestion and edit the buffer
pub fn replace_in_buffer(
    value: Option<Suggestion>,
    editor: &mut Editor,
    output_mode: Option<OutputMode>,
) {
    let Some(Suggestion {
        mut value,
        span,
        append_whitespace,
        ..
    }) = value
    else {
        return;
    };

    let buffer = editor.get_buffer();
    let (raw_start, raw_end) = match output_mode {
        Some(OutputMode::FullBuffer) => (0, buffer.len()),
        Some(OutputMode::ExtendToEnd) => (span.start, buffer.len()),
        Some(OutputMode::SuggestedSpan) | None => (span.start, span.end),
    };

    let end = floor_char_boundary(buffer, raw_end);
    let start = floor_char_boundary(buffer, raw_start).min(end);

    if append_whitespace {
        value.push(' ');
    }

    commit_buffer_replacement(editor, start, end, &value);
}

#[derive(Debug, PartialEq)]
struct AnsiSegment<'a> {
    /// One or more Select Graphic Rendition control sequences.
    /// Note: does NOT include the Control Sequence Introducer ('ESC [') at the beginning.
    escape: Option<&'a str>,
    text: &'a str,
}

struct AnsiEscape {
    /// Index where Control Sequence Introducer ('ESC [') starts
    csi_start: usize,
    /// Index where SGR arguments start. `None` if it ends in the reset attribute
    escape_start: Option<usize>,
    escape_end: usize,
    /// Whether the original sequence contained the reset attribute
    had_reset: bool,
}

const ANSI_SGR_START: &str = "\x1b[";

/// Parse ANSI sequences for setting display attributes in the given string.
///
/// Notes:
/// * The resulting `AnsiSegment`s don't include resets. A reset is implied before every segment.
/// * A single `AnsiSegment` can contain multiple consecutive control sequences.
///
/// Only parses Select Graphic Rendition control sequences, ignoring other ANSI sequencse.
/// Essentially just looks for 'ESC [' followed by /[0-9;]*m/.
fn parse_ansi<'a>(s: &'a str) -> Vec<AnsiSegment<'a>> {
    let mut segments = Vec::new();

    let find_escape_end = |sgr_args_start: usize| {
        let mut escape_start = sgr_args_start;
        let mut contains_reset = false;
        // Whether all digits of the current argument have been 0 so far (this
        // is true for empty arguments too). A 0 (or empty argument) represents
        // the reset attribute.
        let mut all_zeroes = true;
        for (i, c) in s[sgr_args_start..].char_indices() {
            match c {
                'm' => {
                    let csi_start = sgr_args_start - ANSI_SGR_START.len();
                    let escape_end = sgr_args_start + i + 1;
                    if all_zeroes {
                        return Some(AnsiEscape {
                            csi_start,
                            escape_start: None,
                            escape_end,
                            had_reset: true,
                        });
                    } else {
                        return Some(AnsiEscape {
                            csi_start,
                            escape_start: Some(escape_start),
                            escape_end,
                            had_reset: contains_reset,
                        });
                    }
                }
                '0' => {}
                '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => all_zeroes = false,
                ';' => {
                    if all_zeroes {
                        contains_reset = true;
                        escape_start = sgr_args_start + i + 1;
                    }
                    all_zeroes = true;
                }
                _ => return None,
            }
        }
        // No ending "m" to terminate SGR sequence
        None
    };

    let find_escape = |mut search_start: usize| {
        while let Some(i) = s[search_start..].find(ANSI_SGR_START) {
            if let Some(res) = find_escape_end(search_start + i + ANSI_SGR_START.len()) {
                return Some(res);
            } else {
                search_start = search_start + i + ANSI_SGR_START.len();
            }
        }
        None
    };

    let Some(AnsiEscape {
        csi_start,
        mut escape_start,
        mut escape_end,
        had_reset: _,
    }) = find_escape(0)
    else {
        return vec![AnsiSegment {
            escape: None,
            text: s,
        }];
    };
    // The unformatted text at the start, without any ANSI escapes before it
    segments.push(AnsiSegment {
        escape: None,
        text: &s[..csi_start],
    });

    loop {
        while s[escape_end..].starts_with(ANSI_SGR_START) {
            if let Some(AnsiEscape {
                csi_start: _,
                escape_start: next_start,
                escape_end: next_end,
                had_reset,
            }) = find_escape_end(escape_end + ANSI_SGR_START.len())
            {
                if had_reset || escape_start.is_none() {
                    escape_start = next_start;
                }
                escape_end = next_end;
            } else {
                break;
            }
        }

        let escape = escape_start.map(|start| &s[start..escape_end]);
        if let Some(AnsiEscape {
            csi_start,
            escape_start: new_start,
            escape_end: new_end,
            had_reset: _,
        }) = find_escape(escape_end)
        {
            segments.push(AnsiSegment {
                escape,
                text: &s[escape_end..csi_start],
            });
            escape_start = new_start;
            escape_end = new_end;
        } else {
            segments.push(AnsiSegment {
                escape,
                text: &s[escape_end..s.len()],
            });
            break;
        }
    }

    segments
}

/// Style a suggestion to be shown in a completer menu
///
/// * `match_indices` - Indices of graphemes (NOT bytes or chars) that matched the typed text
/// * `match_style` - Style to use for matched characters
pub fn style_suggestion(
    suggestion: &str,
    match_indices: &[usize],
    text_style: &Style,
    match_style: &Style,
    selected_style: Option<&Style>,
) -> String {
    let text_style_prefix = text_style.prefix().to_string();
    let match_style_prefix = match_style.prefix().to_string();
    let selected_prefix = selected_style
        .map(|s| s.prefix().to_string())
        .unwrap_or_default();
    let mut res = String::new();
    let mut offset = 0;
    let ansi_segments = parse_ansi(suggestion);
    for AnsiSegment { escape, text } in ansi_segments {
        if text.is_empty() {
            continue;
        }

        let graphemes = text.graphemes(true).collect::<Vec<_>>();
        let mut prev_matched = false;

        res.push_str(RESET);
        res.push_str(&text_style_prefix);
        res.push_str(&selected_prefix);
        if let Some(escape) = escape {
            res.push_str(ANSI_SGR_START);
            res.push_str(escape);
        }
        for (i, grapheme) in graphemes.iter().enumerate() {
            let is_match = match_indices.contains(&(i + offset));

            if is_match && !prev_matched {
                res.push_str(RESET);
                res.push_str(&text_style_prefix);
                res.push_str(&match_style_prefix);
                if let Some(escape) = escape {
                    res.push_str(ANSI_SGR_START);
                    res.push_str(escape);
                }
            } else if !is_match && prev_matched && i != 0 {
                res.push_str(RESET);
                res.push_str(&text_style_prefix);
                res.push_str(&selected_prefix);
                if let Some(escape) = escape {
                    res.push_str(ANSI_SGR_START);
                    res.push_str(escape);
                }
            }
            res.push_str(grapheme);
            prev_matched = is_match;
        }

        if prev_matched {
            res.push_str(RESET);
        }

        offset += graphemes.len();
    }

    res
}

/// If `match_indices` is given, then returns that. Otherwise, tries to find `typed_text`
/// inside `value`, then returns the indices for that substring.
pub fn get_match_indices<'a>(
    value: &str,
    match_indices: &'a Option<Vec<usize>>,
    typed_text: &str,
) -> Cow<'a, Vec<usize>> {
    if let Some(inds) = match_indices {
        Cow::Borrowed(inds)
    } else {
        let Some(match_pos) = value.to_lowercase().find(&typed_text.to_lowercase()) else {
            // Don't highlight anything if no match
            return Cow::Owned(vec![]);
        };
        let match_len = typed_text.graphemes(true).count();
        Cow::Owned((match_pos..match_pos + match_len).collect())
    }
}

/// Where `truncate_with_ansi` cuts: segments before `segment` are kept whole,
/// then the first `byte` bytes of `segment`, then the suffix.
struct Cut {
    segment: usize,
    /// Always a grapheme boundary within that segment's text.
    byte: usize,
}

/// How one segment sits against the width budget once everything before it
/// has been laid out.
enum Fit {
    /// The segment fits, and the suffix would still fit after it.
    WithDots,
    /// The segment fits, but the suffix after it would not: if anything later
    /// overflows, the cut has to land inside this segment.
    WithoutDots,
    /// The segment itself does not fit.
    Overflows,
}

fn fit(current_width: usize, segment_width: usize, suffix_width: usize, max_width: usize) -> Fit {
    let end = current_width + segment_width;
    if end > max_width {
        Fit::Overflows
    } else if end + suffix_width > max_width {
        Fit::WithoutDots
    } else {
        Fit::WithDots
    }
}

/// Byte length of the longest prefix of `text` whose display width is at most `budget`.
///
/// Walks graphemes so a wide or combining character is never split; a
/// grapheme that would overshoot the budget stops the walk.
fn prefix_len_within_width(text: &str, budget: usize) -> usize {
    let mut remaining = budget;
    let mut end = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        let width = grapheme.width();
        if width > remaining {
            break;
        }
        end = start + grapheme.len();
        remaining -= width;
    }
    end
}

/// Truncate a string with ANSI escapes to the given max width, which must be >=3.
///
/// If `s` is longer than `max_width`, the resulting string will end in "..."
/// and have width at most `max_width`.
pub(crate) fn truncate_with_ansi(s: &str, max_width: usize) -> Cow<'_, str> {
    let suffix_width = TRUNCATION_SUFFIX.width();
    let segments = parse_ansi(s);

    // The cut lands at the first place the suffix stops fitting: inside the
    // first `WithoutDots` segment after a clean boundary, or failing that
    // inside the segment that overflows. `pending` holds the former until we
    // know whether anything overflows at all; a `WithDots` segment resets it
    // since the suffix fits cleanly after that one again.
    let mut current_width = 0;
    let mut pending: Option<Cut> = None;
    let mut cut: Option<Cut> = None;
    for (i, segment) in segments.iter().enumerate() {
        let segment_width = segment.text.width();
        let budget = max_width
            .saturating_sub(current_width)
            .saturating_sub(suffix_width);
        match fit(current_width, segment_width, suffix_width, max_width) {
            Fit::WithDots => pending = None,
            Fit::WithoutDots => {
                if pending.is_none() {
                    pending = Some(Cut {
                        segment: i,
                        byte: prefix_len_within_width(segment.text, budget),
                    })
                }
            }
            Fit::Overflows => {
                cut = Some(pending.take().unwrap_or_else(|| Cut {
                    segment: i,
                    byte: prefix_len_within_width(segment.text, budget),
                }));
                break;
            }
        }
        current_width += segment_width;
    }

    let Some(Cut { segment, byte }) = cut else {
        return Cow::Borrowed(s);
    };
    // `segment` is an index the loop above produced, so both lookups hold; if
    // they ever did not, an untruncated string beats a panic while painting.
    let Some((keep, rest)) = segments.split_at_checked(segment) else {
        return Cow::Borrowed(s);
    };
    let Some(last) = rest.first() else {
        return Cow::Borrowed(s);
    };

    let mut res = String::new();
    for (i, segment) in keep.iter().enumerate() {
        if let Some(escape) = segment.escape {
            res.push_str(ANSI_SGR_START);
            res.push_str(escape);
        } else if i > 0 {
            // No need to put a RESET at the beginning of the string
            res.push_str(RESET);
        }
        res.push_str(segment.text);
    }
    // The cut segment's escape is emitted even when none of its text survives,
    // so the suffix carries its style.
    if let Some(escape) = last.escape {
        res.push_str(ANSI_SGR_START);
        res.push_str(escape);
    } else if segment > 0 {
        res.push_str(RESET);
    }
    res.push_str(last.text.get(..byte).unwrap_or(""));
    res.push_str(TRUNCATION_SUFFIX);
    Cow::Owned(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "helix")]
    use crate::PromptHelixMode;
    use crate::{EditCommand, LineBuffer, PromptEditMode, PromptViMode, Span};
    use nu_ansi_term::Color;
    use rstest::rstest;

    /// A caret cursor rests on the last grapheme of the word, so completion must
    /// count that grapheme as part of the word instead of stranding it after the
    /// replacement (`foo` + `foobar` used to yield `foobaro`).
    #[rstest]
    #[cfg_attr(
        feature = "helix",
        case::helix_normal(PromptEditMode::Helix(PromptHelixMode::Normal), "foo", "foobar")
    )]
    #[case::vi_normal(PromptEditMode::Vi(PromptViMode::Normal), "foo", "foobar")]
    // The covered grapheme is multi-byte: stepping by bytes would split it.
    #[cfg_attr(
        feature = "helix",
        case::multibyte(PromptEditMode::Helix(PromptHelixMode::Normal), "café", "cafétería")
    )]
    fn completion_covers_the_caret_grapheme(
        #[case] mode: PromptEditMode,
        #[case] buffer: &str,
        #[case] expected: &str,
    ) {
        use crate::{menu::MenuSettings, Completer, DefaultCompleter};

        let mut completer = DefaultCompleter::default();
        completer.insert(vec![expected.to_string()]);

        let mut editor = Editor::default();
        let mut lb = LineBuffer::new();
        lb.set_buffer(buffer.to_string());
        editor.set_line_buffer(lb, UndoBehavior::CreateUndoPoint);
        editor.set_edit_mode(mode);

        let mut saved = None;
        let (input, pos) = resolve_completer_input(&editor, &mut saved, &MenuSettings::default());
        let suggestions = completer.complete(&input, pos);
        replace_in_buffer(
            suggestions.suggestions().first().cloned(),
            &mut editor,
            None,
        );

        assert_eq!(editor.get_buffer(), expected);
    }

    /// Insert mode has no caret widening, so it must keep the plain insertion
    /// point — the guard against the fix leaking into bar-cursor modes.
    #[test]
    fn completion_point_is_untouched_for_bar_cursors() {
        let mut editor = Editor::default();
        let mut lb = LineBuffer::new();
        lb.set_buffer("foo".to_string());
        editor.set_line_buffer(lb, UndoBehavior::CreateUndoPoint);
        editor.set_edit_mode(PromptEditMode::Vi(PromptViMode::Insert));

        assert_eq!(editor.completion_point(), editor.insertion_point());
    }

    #[test]
    fn parse_row_test() {
        let input = "search:6";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(6));
        assert_eq!(res.marker, Some(":6"));
    }

    #[cfg(feature = "bashisms")]
    #[test]
    fn handles_multi_byte_char_as_marker_and_number() {
        let buffer = "searchは6";
        let parse_result = parse_selection_char(buffer, 'は');

        assert_eq!(parse_result.remainder, "search");
        assert_eq!(parse_result.index, Some(6));
        assert_eq!(parse_result.marker, Some("は6"));
    }

    #[cfg(feature = "bashisms")]
    #[test]
    fn handles_multi_byte_char_as_double_marker() {
        let buffer = "Testはは";
        let parse_result = parse_selection_char(buffer, 'は');

        assert_eq!(parse_result.remainder, "Test");
        assert_eq!(parse_result.index, Some(0));
        assert_eq!(parse_result.marker, Some("はは"));
        assert!(matches!(parse_result.action, ParseAction::LastCommand));
    }

    #[cfg(feature = "bashisms")]
    #[test]
    fn handles_multi_byte_char_as_remainder() {
        let buffer = "Testは!!";
        let parse_result = parse_selection_char(buffer, '!');

        assert_eq!(parse_result.remainder, "Testは");
        assert_eq!(parse_result.index, Some(0));
        assert_eq!(parse_result.marker, Some("!!"));
        assert!(matches!(parse_result.action, ParseAction::LastCommand));
    }

    #[cfg(feature = "bashisms")]
    #[test]
    fn parse_double_char() {
        let input = "search!!";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some("!!"));
        assert!(matches!(res.action, ParseAction::LastCommand));
    }

    #[cfg(feature = "bashisms")]
    #[test]
    fn parse_last_token() {
        let input = "!$";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some("!$"));
        assert!(matches!(res.action, ParseAction::LastToken));
    }

    #[test]
    fn parse_row_other_marker_test() {
        let input = "search?9";
        let res = parse_selection_char(input, '?');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(9));
        assert_eq!(res.marker, Some("?9"));
    }

    #[test]
    fn parse_row_double_test() {
        let input = "ls | where:16";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "ls | where");
        assert_eq!(res.index, Some(16));
        assert_eq!(res.marker, Some(":16"));
    }

    #[test]
    fn parse_row_empty_test() {
        let input = ":10";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, Some(10));
        assert_eq!(res.marker, Some(":10"));
    }

    #[test]
    fn parse_row_fake_indicator_test() {
        let input = "let a: another :10";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "let a: another ");
        assert_eq!(res.index, Some(10));
        assert_eq!(res.marker, Some(":10"));
    }

    #[test]
    fn parse_row_no_number_test() {
        let input = "let a: another:";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "let a: another");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some(":"));
    }

    #[test]
    fn parse_empty_buffer_test() {
        let input = "";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, None);
        assert_eq!(res.marker, None);
    }

    #[test]
    fn parse_negative_direction() {
        let input = "!-2";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, Some(2));
        assert_eq!(res.marker, Some("!-2"));
        assert!(matches!(res.action, ParseAction::BackwardSearch));
    }

    #[rstest]
    #[case::inserted_word("this is a new string", "this is a string", 10, "new ")]
    #[case::appended("this is a new string", "this is", 7, " a new string")]
    #[case::new_shorter("this is the", "this is the original", 11, "")]
    #[case::inserted_inside_parens("let a = (insert) | ", "let a = () | ", 9, "insert")]
    #[case::tail_differs("this is a new another", "this is a string", 10, "new another")]
    #[case::inserted_words(
        "this is a new something string",
        "this is a string",
        10,
        "new something "
    )]
    #[case::empty_old("this new another", "", 0, "this new another")]
    #[case::nothing_shared("this new another", "complete different string", 0, "this new another")]
    #[case::equal("this new another", "this new another", 16, "")]
    #[case::multibyte_diff("ｎｕｓｈｅｌｌ", "ｎｕｌｌ", 6, "ｓｈｅ")]
    #[case::multibyte_prefix("héllo wörld", "héllo", 6, " wörld")]
    #[case::repeat("ee", "e", 1, "e")]
    #[case::repeat_twice("eee", "e", 1, "ee")]
    #[case::old_is_prefix_and_reappears("abcb", "ab", 2, "cb")]
    fn string_difference_is_the_text_typed_after_the_prefix(
        #[case] new: &str,
        #[case] old: &str,
        #[case] start: usize,
        #[case] diff: &str,
    ) {
        assert_eq!(string_difference(new, old), (start, diff));
    }

    #[rstest]
    #[case::ascii(vec!["nushell", "null"], 2)]
    #[case::non_ascii(vec!["ｎｕｓｈｅｌｌ", "ｎｕｌｌ"], 6)]
    // https://github.com/nushell/nushell/pull/16765#issuecomment-3384411809
    #[case::unsorted(vec!["a", "b", "ab"], 0)]
    #[case::should_be_case_sensitive(vec!["a", "A"], 0)]
    #[case::first_suggestion_longest(vec!["foobar", "foo"], 3)]
    fn test_derive_common_prefix(#[case] input: Vec<&str>, #[case] expected: usize) {
        let input: Vec<_> = input
            .into_iter()
            .map(|s| Suggestion {
                value: s.into(),
                ..Default::default()
            })
            .collect();
        // Shared default span, empty buffer: gate reduces to "non-empty prefix?"
        let partial = derive_common_prefix("", &input);

        match expected {
            0 => assert!(partial.is_none()),
            len => assert_eq!(partial.unwrap().insert.len(), len),
        }
    }

    /// Verifies fallback to built-in derivation without completer-supplied Partial
    #[test]
    fn common_prefix_falls_back_to_derivation_without_a_completer_supplied_partial() {
        let mut editor = Editor::default();
        editor.set_buffer("ab".to_string(), UndoBehavior::CreateUndoPoint);

        // Mirrors demo example: "ab" + Tab -> longest prefix "abaaa"
        let values: Suggestions = ["abaaacas", "abaaac", "abaaaxyc", "abaaarabc"]
            .into_iter()
            .map(|value| Suggestion {
                value: value.to_string(),
                span: Span::new(0, 2),
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into();
        let display = CompletionDisplay::new(
            values,
            None,
            &[],
            CompletionOrigin::new(editor.get_buffer(), editor.insertion_point()),
        );

        assert!(display.common_prefix(&mut editor));
        assert_eq!(editor.get_buffer(), "abaaa");
    }

    #[rstest]
    #[case("foobar", 6, None, InputMode::CursorPrefix, "foobar", 6)]
    #[case("foo\r\nbar", 5, None, InputMode::CursorPrefix, "foo\r\n", 5)]
    #[case("foo\nbar", 4, None, InputMode::CursorPrefix, "foo\n", 4)]
    #[case("foobar", 6, None, InputMode::Diff, "", 6)]
    #[case("foobar", 3, Some("foobar"), InputMode::Diff, "", 3)]
    #[case("foobar", 6, Some("foo"), InputMode::Diff, "bar", 6)]
    #[case("foobar", 6, Some("for"), InputMode::Diff, "oba", 5)]
    #[case("foobar baz", 3, None, InputMode::FullBuffer, "foobar baz", 3)]
    fn test_completer_input(
        #[case] buffer: String,
        #[case] insertion_point: usize,
        #[case] prev_input: Option<&str>,
        #[case] input_mode: InputMode,
        #[case] output: String,
        #[case] pos: usize,
    ) {
        assert_eq!(
            (output, pos),
            completer_input(&buffer, insertion_point, prev_input, input_mode)
        )
    }

    #[rstest]
    #[case("foobar baz", 6, "foobleh baz", 7, "bleh", 3, 6)]
    #[case("foobar baz", 6, "foo baz", 3, "", 3, 6)]
    #[case("foobar baz", 10, "foobleh", 7, "bleh", 3, 1000)]
    fn test_replace_in_buffer(
        #[case] orig_buffer: &str,
        #[case] orig_insertion_point: usize,
        #[case] new_buffer: &str,
        #[case] new_insertion_point: usize,
        #[case] value: String,
        #[case] start: usize,
        #[case] end: usize,
    ) {
        let mut editor = Editor::default();
        let mut line_buffer = LineBuffer::new();
        line_buffer.set_buffer(orig_buffer.to_owned());
        line_buffer.set_insertion_point(orig_insertion_point);
        editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
        replace_in_buffer(
            Some(Suggestion {
                value,
                span: Span::new(start, end),
                ..Default::default()
            }),
            &mut editor,
            None,
        );
        assert_eq!(new_buffer, editor.get_buffer());
        assert_eq!(new_insertion_point, editor.insertion_point());

        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(orig_buffer, editor.get_buffer());
        assert_eq!(orig_insertion_point, editor.insertion_point());
    }

    #[rstest]
    #[case::full_buffer(
        "old content",
        11,
        "new",
        3,
        "new",
        Span::new(0, 0),
        OutputMode::FullBuffer
    )]
    #[case::extend_to_end(
        "hello world",
        11,
        "hello rust",
        10,
        "rust",
        Span::new(6, 8),
        OutputMode::ExtendToEnd
    )]
    fn test_replace_in_buffer_with_output_mode(
        #[case] orig_buffer: &str,
        #[case] orig_insertion_point: usize,
        #[case] new_buffer: &str,
        #[case] new_insertion_point: usize,
        #[case] value: String,
        #[case] span: Span,
        #[case] output_mode: OutputMode,
    ) {
        let mut editor = Editor::default();
        let mut line_buffer = LineBuffer::new();
        line_buffer.set_buffer(orig_buffer.to_owned());
        line_buffer.set_insertion_point(orig_insertion_point);
        editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
        replace_in_buffer(
            Some(Suggestion {
                value,
                span,
                ..Default::default()
            }),
            &mut editor,
            Some(output_mode),
        );
        assert_eq!(new_buffer, editor.get_buffer());
        assert_eq!(new_insertion_point, editor.insertion_point());

        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(orig_buffer, editor.get_buffer());
        assert_eq!(orig_insertion_point, editor.insertion_point());
    }

    #[rstest]
    #[case::plain("Foo", vec![AnsiSegment { escape: None, text: "Foo" }])]
    #[case::unterminated("\x1b[", vec![AnsiSegment { escape: None, text: "\x1b[" }])]
    #[case::invalid(
        "\x1b[\x1b[mFoo",
        vec![
            AnsiSegment { escape: None, text: "\x1b[" },
            AnsiSegment { escape: None, text: "Foo" },
        ]
    )]
    #[case::no_args_reset(
        "\x1b[3m\x1b[m\x1b[2mFoo",
        vec![
            AnsiSegment { escape: None, text: "" },
            AnsiSegment { escape: Some("2m"), text: "Foo" },
        ]
    )]
    #[case::empty_reset_with_args_afterwards(
        "\x1b[3m\x1b[1;;20mFoo",
        vec![
            AnsiSegment { escape: None, text: "" },
            AnsiSegment { escape: Some("20m"), text: "Foo" },
        ]
    )]
    #[case::empty_reset_without_args_afterwards(
        "\x1b[3m\x1b[1;mFoo",
        vec![
            AnsiSegment { escape: None, text: "" },
            AnsiSegment { escape: None, text: "Foo" },
        ]
    )]
    #[case::zero_reset_without_args_afterwards(
        "\x1b[3m\x1b[10;0mFoo",
        vec![
            AnsiSegment { escape: None, text: "" },
            AnsiSegment { escape: None, text: "Foo" },
        ]
    )]
    #[case::multiple(
        "Foo\x1b[1;0;2m\x1b[2;3m\x1b[Bar\x1b[1;2m\x1b[2;3mBaz",
        vec![
            AnsiSegment { escape: None, text: "Foo" },
            AnsiSegment { escape: Some("2m\x1b[2;3m"), text: "\x1b[Bar" },
            AnsiSegment { escape: Some("1;2m\x1b[2;3m"), text: "Baz" },
        ]
    )]
    fn test_parse_ansi(#[case] s: &str, #[case] expected: Vec<AnsiSegment>) {
        assert_eq!(parse_ansi(s), expected);
    }

    #[test]
    fn style_fuzzy_suggestion() {
        let text_style = Style::new().fg(Color::Red);
        let match_style = Style::new().underline();
        let selected_style = Style::new().underline();
        let style1 = Style::new().on(Color::Blue);
        let style2 = Style::new().on(Color::Green);

        let expected = format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
            RESET,
            text_style.prefix(),
            selected_style.prefix(),
            style1.prefix(),
            "ab",
            RESET,
            text_style.prefix(),
            match_style.prefix(),
            style1.prefix(),
            "汉",
            RESET,
            text_style.prefix(),
            selected_style.prefix(),
            style1.prefix(),
            "d",
            RESET,
            text_style.prefix(),
            selected_style.prefix(),
            style2.prefix(),
            RESET,
            text_style.prefix(),
            match_style.prefix(),
            style2.prefix(),
            "y̆👩🏾",
            RESET,
            text_style.prefix(),
            selected_style.prefix(),
            style2.prefix(),
            "e",
            RESET,
            text_style.prefix(),
            selected_style.prefix(),
            "b@",
            RESET,
            text_style.prefix(),
            match_style.prefix(),
            "r",
            RESET,
        );
        let match_indices = &[
            2, // 汉
            4, 5, // y̆👩🏾
            9, // r
        ];
        assert_eq!(
            expected,
            style_suggestion(
                &format!("{}{}{}", style1.paint("ab汉d"), style2.paint("y̆👩🏾e"), "b@r"),
                match_indices,
                &text_style,
                &match_style,
                Some(&selected_style),
            )
        );
    }

    #[test]
    fn style_fuzzy_suggestion_out_of_bounds() {
        let text_style = Style::new().on(Color::Blue).bold();
        let match_style = Style::new().underline();

        let expected = format!(
            "{}{}{}{}{}{}{}{}",
            RESET,
            text_style.prefix(),
            "go",
            RESET,
            text_style.prefix(),
            match_style.prefix(),
            "o",
            RESET,
        );
        assert_eq!(
            expected,
            style_suggestion("goo", &[2, 3, 4, 6], &text_style, &match_style, None)
        );
    }

    #[rstest]
    #[case::no_ansi_shorter("asdf", 5, "asdf")]
    #[case::with_ansi_shorter(
        "\x1b[1;2;3;ma\x1b[1;15;ms\x1b[1;md\x1b[1;mf",
        5,
        "\x1b[1;2;3;ma\x1b[1;15;ms\x1b[1;md\x1b[1;mf"
    )]
    // Ｈ has width 2
    #[case::no_ansi_one_longer("asdfＨ", 5, "as...")]
    #[case::no_ansi_result_thinner_than_max("aＨＨＨ", 5, "a...")]
    #[case::with_ansi_exact_width("\x1b[2masd\x1b[2;3;mＨ", 5, "\x1b[2masd\x1b[2;3;mＨ")]
    #[case::no_ansi_nothing_left("foobar", 3, "...")]
    #[case::trunc_with_short_segments("foobar\x1b[1;ma\x1b[2;mb\x1b[3;mc", 8, "fooba...")]
    #[case::trunc_with_long_segment("foo\x1b[1;mBarbaz\x1b[2;mExtra", 8, "foo\x1b[0mBa...")]
    // The cases below pin which segment the cut lands in and where inside it,
    // with well-formed SGR escapes (no trailing `;`, which the parser reads as a reset).
    #[case::style_survives_the_cut("\x1b[1mabcdef", 4, "\x1b[1ma...")]
    #[case::cut_on_a_segment_boundary_keeps_its_escape("ab\x1b[1mcd\x1b[2mef", 5, "ab\x1b[1m...")]
    #[case::earlier_segment_fits_only_without_dots(
        "ab\x1b[1mcd\x1b[2mefgh",
        7,
        "ab\x1b[1mcd\x1b[2m..."
    )]
    #[case::cut_lands_in_the_first_of_two_without_dots(
        "ab\x1b[1mcd\x1b[2me\x1b[3mfghij",
        6,
        "ab\x1b[1mc..."
    )]
    #[case::overflow_segment_has_no_room_left(
        "abc\x1b[1mde\x1b[2mfghij",
        8,
        "abc\x1b[1mde\x1b[2m..."
    )]
    #[case::wide_grapheme_starts_the_overflow_segment(
        "ab\x1b[1m\u{ff28}\u{ff28}\x1b[2mcd",
        5,
        "ab\x1b[1m..."
    )]
    #[case::empty_text_segment_between_escapes("a\x1b[1m\x1b[2mbcdefg", 5, "a\x1b[1m\x1b[2mb...")]
    #[case::reset_inside_the_kept_part("\x1b[1mab\x1b[0mcdef", 5, "\x1b[1mab\x1b[0m...")]
    #[case::reset_after_the_cut("\x1b[1mab\x1b[0mcdef", 4, "\x1b[1ma...")]
    #[case::max_width_below_the_suffix("abcdef", 2, "...")]
    #[case::max_width_zero("abcdef", 0, "...")]
    fn test_truncate_with_ansi(
        #[case] value: &str,
        #[case] max_width: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(expected, truncate_with_ansi(value, max_width));
    }
}
