//! Collection of common functions that can be used to create menus
use std::borrow::Cow;
use unicase::UniCase;

use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use nu_ansi_term::{ansi::RESET, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{Editor, Suggestion, UndoBehavior};

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
                    }
                }
                #[cfg(feature = "bashisms")]
                Some(&x) if x == '$' => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + 2]),
                        action: ParseAction::LastToken,
                        prefix: None,
                    }
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
                        if c.is_ascii_digit() {
                            let c = c.to_digit(10).expect("already checked if is a digit");
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
                    }
                }
                None => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..buffer.len()]),
                        action: ParseAction::ForwardSearch,
                        prefix: Some(&buffer[index..buffer.len()]),
                    }
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

/// Finds index for the common string in a list of suggestions
pub fn find_common_string(values: &[Suggestion]) -> Option<(&Suggestion, usize)> {
    let first_suggestion = values.first()?;
    let max_len = first_suggestion.value.len();

    let index = values
        .iter()
        .skip(1)
        .fold_while(max_len, |cumulated_min, current_suggestion| {
            let new_common_prefix_len = first_suggestion
                .value
                .char_indices()
                .zip(current_suggestion.value.chars())
                .find_map(|((idx, lhs), rhs)| (rhs != lhs).then_some(idx))
                .unwrap_or(current_suggestion.value.len());
            if new_common_prefix_len == 0 {
                Done(0)
            } else {
                Continue(cumulated_min.min(new_common_prefix_len))
            }
        });

    Some((first_suggestion, index.into_inner()))
}

/// Finds different string between two strings
///
/// ## Example usage
/// ```
/// use reedline::menu_functions::string_difference;
///
/// let new_string = "this is a new string";
/// let old_string = "this is a string";
///
/// let res = string_difference(new_string, old_string);
/// assert_eq!(res, (10, "new "));
/// ```
pub fn string_difference<'a>(new_string: &'a str, old_string: &str) -> (usize, &'a str) {
    if old_string.is_empty() {
        return (0, new_string);
    }

    let old_chars = old_string.char_indices().collect::<Vec<(usize, char)>>();
    let new_chars = new_string.char_indices().collect::<Vec<(usize, char)>>();

    let (_, start, end) = new_chars.iter().enumerate().fold(
        (0, None, None),
        |(old_char_index, start, end), (new_char_index, (_, c))| {
            let equal = if start.is_some() {
                if (old_chars.len() - old_char_index) == (new_chars.len() - new_char_index) {
                    let new_iter = new_chars.iter().skip(new_char_index);
                    let old_iter = old_chars.iter().skip(old_char_index);

                    new_iter
                        .zip(old_iter)
                        .all(|((_, new), (_, old))| new == old)
                } else {
                    false
                }
            } else {
                old_char_index == new_char_index && *c == old_chars[old_char_index].1
            };

            if equal {
                let old_char_index = (old_char_index + 1).min(old_chars.len() - 1);

                let end = match (start, end) {
                    (Some(_), Some(_)) => end,
                    (Some(_), None) => Some(new_char_index),
                    _ => None,
                };

                (old_char_index, start, end)
            } else {
                let start = match start {
                    Some(_) => start,
                    None => Some(new_char_index),
                };

                (old_char_index, start, end)
            }
        },
    );

    // Convert char index to byte index
    let start = start.map(|i| new_chars[i].0);
    let end = end.map(|i| new_chars[i].0);

    match (start, end) {
        (Some(start), Some(end)) => (start, &new_string[start..end]),
        (Some(start), None) => (start, &new_string[start..]),
        (None, None) => (new_string.len(), ""),
        (None, Some(_)) => unreachable!(),
    }
}

/// Get the part of the line that should be given as input to the completer, as well
/// as the index of the end of that piece of text
///
/// `prev_input` is the text in the buffer when the menu was activated. Needed if only_buffer_difference is true
pub fn completer_input(
    buffer: &str,
    insertion_point: usize,
    prev_input: Option<&str>,
    only_buffer_difference: bool,
) -> (String, usize) {
    if only_buffer_difference {
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
    } else {
        // TODO previously, all but the list menu replaced newlines with spaces here
        // The completers should be adapted to account for this, and tests need to be added
        (buffer[..insertion_point].to_owned(), insertion_point)
    }
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

/// Helper to accept a completion suggestion and edit the buffer
pub fn replace_in_buffer(value: Option<Suggestion>, editor: &mut Editor) {
    if let Some(Suggestion {
        mut value,
        span,
        append_whitespace,
        ..
    }) = value
    {
        let end = floor_char_boundary(editor.get_buffer(), span.end);
        let start = floor_char_boundary(editor.get_buffer(), span.start).min(end);
        if append_whitespace {
            value.push(' ');
        }

        let mut line_buffer = editor.line_buffer().clone();
        line_buffer.replace_range(start..end, &value);
        let mut offset = line_buffer.insertion_point();
        offset = offset.saturating_add(value.len());
        offset = offset.saturating_sub(end.saturating_sub(start));
        line_buffer.set_insertion_point(offset);
        editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
    }
}

/// Helper for `Menu::can_partially_complete`
pub fn can_partially_complete(values: &[Suggestion], editor: &mut Editor) -> bool {
    if let Some((Suggestion { value, span, .. }, index)) = find_common_string(values) {
        let matching = &value[0..index];
        let end = floor_char_boundary(editor.get_buffer(), span.end);
        let start = floor_char_boundary(editor.get_buffer(), span.start).min(end);

        // make sure that the partial completion does not overwrite user entered input
        let entered_input = &editor.get_buffer()[start..end];
        let extends_input = UniCase::new(matching)
            .to_folded_case()
            .contains(&UniCase::new(entered_input).to_folded_case())
            && matching != entered_input;

        if !matching.is_empty() && extends_input {
            let mut line_buffer = editor.line_buffer().clone();
            line_buffer.replace_range(start..end, matching);

            let offset = if matching.len() < (end - start) {
                line_buffer
                    .insertion_point()
                    .saturating_sub((end - start) - matching.len())
            } else {
                line_buffer.insertion_point() + matching.len() - (end - start)
            };

            line_buffer.set_insertion_point(offset);
            editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);

            true
        } else {
            false
        }
    } else {
        false
    }
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

/// Truncate a string with ANSI escapes to the given max width, which must be >=3.
///
/// If `s` is longer than `max_width`, the resulting string will end in "..."
/// and have width at most `max_width`.
pub(crate) fn truncate_with_ansi(s: &str, max_width: usize) -> Cow<'_, str> {
    let trunc_suffix = "...";
    let suffix_width = trunc_suffix.width();

    let ansi_segments = parse_ansi(s);
    let mut curr_width = 0;
    let mut should_trunc = false;
    let mut max_ind_trunc = 0;
    let mut trunc_grapheme_ind = 0;
    for (i, segment) in ansi_segments.iter().enumerate() {
        let segment_width = segment.text.width();

        should_trunc = curr_width + segment_width > max_width;

        let too_long_with_dots = curr_width + segment_width + suffix_width > max_width;
        if !too_long_with_dots {
            max_ind_trunc = i + 1;
        }

        if should_trunc || too_long_with_dots {
            let mut allowed_width = max_width
                .saturating_sub(curr_width)
                .saturating_sub(suffix_width);
            for (ind, grapheme) in segment.text.grapheme_indices(true) {
                let grapheme_width = grapheme.width();
                if grapheme_width > allowed_width {
                    break;
                }
                trunc_grapheme_ind = ind + grapheme.len();
                allowed_width = allowed_width.saturating_sub(grapheme_width);
            }
            if should_trunc {
                break;
            }
        }

        curr_width += segment_width;
    }

    if should_trunc {
        let mut res = String::new();
        for (i, segment) in ansi_segments[0..max_ind_trunc].iter().enumerate() {
            if let Some(escape) = segment.escape {
                res.push_str(ANSI_SGR_START);
                res.push_str(escape);
            } else if i > 0 {
                // No need to put a RESET at the beginning of the string
                res.push_str(RESET);
            }
            res.push_str(segment.text);
        }
        let last = &ansi_segments[max_ind_trunc];
        if let Some(escape) = last.escape {
            res.push_str(ANSI_SGR_START);
            res.push_str(escape);
        } else if max_ind_trunc > 0 {
            res.push_str(RESET);
        }
        res.push_str(&last.text[0..trunc_grapheme_ind]);
        res.push_str(trunc_suffix);
        Cow::Owned(res)
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditCommand, LineBuffer, Span};
    use nu_ansi_term::Color;
    use rstest::rstest;

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

    #[test]
    fn string_difference_test() {
        let new_string = "this is a new string";
        let old_string = "this is a string";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (10, "new "));
    }

    #[test]
    fn string_difference_new_larger() {
        let new_string = "this is a new string";
        let old_string = "this is";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (7, " a new string"));
    }

    #[test]
    fn string_difference_new_shorter() {
        let new_string = "this is the";
        let old_string = "this is the original";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (11, ""));
    }

    #[test]
    fn string_difference_inserting() {
        let new_string = "let a = (insert) | ";
        let old_string = "let a = () | ";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (9, "insert"));
    }

    #[test]
    fn string_difference_longer_string() {
        let new_string = "this is a new another";
        let old_string = "this is a string";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (10, "new another"));
    }

    #[test]
    fn string_difference_start_same() {
        let new_string = "this is a new something string";
        let old_string = "this is a string";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (10, "new something "));
    }

    #[test]
    fn string_difference_empty_old() {
        let new_string = "this new another";
        let old_string = "";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (0, "this new another"));
    }

    #[test]
    fn string_difference_very_difference() {
        let new_string = "this new another";
        let old_string = "complete different string";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (0, "this new another"));
    }

    #[test]
    fn string_difference_both_equal() {
        let new_string = "this new another";
        let old_string = "this new another";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (16, ""));
    }

    #[test]
    fn string_difference_with_non_ansi() {
        let new_string = "ｎｕｓｈｅｌｌ";
        let old_string = "ｎｕｌｌ";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (6, "ｓｈｅ"));
    }

    #[test]
    fn string_difference_with_repeat() {
        let new_string = "ee";
        let old_string = "e";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (1, "e"));
    }

    #[rstest]
    #[case::ascii(vec!["nushell", "null"], 2)]
    #[case::non_ascii(vec!["ｎｕｓｈｅｌｌ", "ｎｕｌｌ"], 6)]
    // https://github.com/nushell/nushell/pull/16765#issuecomment-3384411809
    #[case::unsorted(vec!["a", "b", "ab"], 0)]
    #[case::should_be_case_sensitive(vec!["a", "A"], 0)]
    #[case::first_suggestion_longest(vec!["foobar", "foo"], 3)]
    fn test_find_common_string(#[case] input: Vec<&str>, #[case] expected: usize) {
        let input: Vec<_> = input
            .into_iter()
            .map(|s| Suggestion {
                value: s.into(),
                ..Default::default()
            })
            .collect();
        let (_, len) = find_common_string(&input).unwrap();

        assert!(len == expected);
    }

    #[rstest]
    #[case("foobar", 6, None, false, "foobar", 6)]
    #[case("foo\r\nbar", 5, None, false, "foo\r\n", 5)]
    #[case("foo\nbar", 4, None, false, "foo\n", 4)]
    #[case("foobar", 6, None, true, "", 6)]
    #[case("foobar", 3, Some("foobar"), true, "", 3)]
    #[case("foobar", 6, Some("foo"), true, "bar", 6)]
    #[case("foobar", 6, Some("for"), true, "oba", 5)]
    fn test_completer_input(
        #[case] buffer: String,
        #[case] insertion_point: usize,
        #[case] prev_input: Option<&str>,
        #[case] only_buffer_difference: bool,
        #[case] output: String,
        #[case] pos: usize,
    ) {
        assert_eq!(
            (output, pos),
            completer_input(&buffer, insertion_point, prev_input, only_buffer_difference)
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
    fn test_truncate_with_ansi(
        #[case] value: &str,
        #[case] max_width: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(expected, truncate_with_ansi(value, max_width));
    }
}
