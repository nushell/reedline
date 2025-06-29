//! Collection of common functions that can be used to create menus
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
pub fn parse_selection_char(buffer: &str, marker: char) -> ParseResult {
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
    let mut action = ParseAction::ForwardSearch;
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
                    while let Some(&c) = input.peek() {
                        if c == '-' {
                            let _ = input.next();
                            size += 1;
                            action = ParseAction::BackwardSearch;
                        } else if c.is_ascii_digit() {
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
                        action,
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
        action,
        prefix: None,
    }
}

/// Finds index for the common string in a list of suggestions
pub fn find_common_string(values: &[Suggestion]) -> (Option<&Suggestion>, Option<usize>) {
    let first = values.iter().next();

    let index = first.and_then(|first| {
        values.iter().skip(1).fold(None, |index, suggestion| {
            if suggestion.value.starts_with(&first.value) {
                Some(first.value.len())
            } else {
                first
                    .value
                    .char_indices()
                    .zip(suggestion.value.char_indices())
                    .find(|((_, mut lhs), (_, mut rhs))| {
                        lhs.make_ascii_lowercase();
                        rhs.make_ascii_lowercase();

                        lhs != rhs
                    })
                    .map(|((new_index, _), _)| match index {
                        Some(index) => {
                            if index <= new_index {
                                index
                            } else {
                                new_index
                            }
                        }
                        None => new_index,
                    })
            }
        })
    });

    (first, index)
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

/// Helper to accept a completion suggestion and edit the buffer
pub fn replace_in_buffer(value: Option<Suggestion>, editor: &mut Editor) {
    if let Some(Suggestion {
        mut value,
        span,
        append_whitespace,
        ..
    }) = value
    {
        let start = span.start.min(editor.line_buffer().len());
        let end = span.end.min(editor.line_buffer().len());
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
    if let (Some(Suggestion { value, span, .. }), Some(index)) = find_common_string(values) {
        let index = index.min(value.len());
        let matching = &value[0..index];

        // make sure that the partial completion does not overwrite user entered input
        let extends_input = matching.starts_with(&editor.get_buffer()[span.start..span.end])
            && matching != &editor.get_buffer()[span.start..span.end];

        if !matching.is_empty() && extends_input {
            let mut line_buffer = editor.line_buffer().clone();
            line_buffer.replace_range(span.start..span.end, matching);

            let offset = if matching.len() < (span.end - span.start) {
                line_buffer
                    .insertion_point()
                    .saturating_sub((span.end - span.start) - matching.len())
            } else {
                line_buffer.insertion_point() + matching.len() - (span.end - span.start)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditCommand, LineBuffer, Span};
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

    #[test]
    fn find_common_string_with_ansi() {
        use crate::Span;

        let input: Vec<_> = ["nushell", "null"]
            .into_iter()
            .map(|s| Suggestion {
                value: s.into(),
                description: None,
                style: None,
                extra: None,
                span: Span::new(0, s.len()),
                append_whitespace: false,
            })
            .collect();
        let res = find_common_string(&input);

        assert!(matches!(res, (Some(elem), Some(2)) if elem == &input[0]));
    }

    #[test]
    fn find_common_string_with_non_ansi() {
        use crate::Span;

        let input: Vec<_> = ["ｎｕｓｈｅｌｌ", "ｎｕｌｌ"]
            .into_iter()
            .map(|s| Suggestion {
                value: s.into(),
                description: None,
                style: None,
                extra: None,
                span: Span::new(0, s.len()),
                append_whitespace: false,
            })
            .collect();
        let res = find_common_string(&input);

        assert!(matches!(res, (Some(elem), Some(6)) if elem == &input[0]));
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
                description: None,
                style: None,
                extra: None,
                span: Span::new(start, end),
                append_whitespace: false,
            }),
            &mut editor,
        );
        assert_eq!(new_buffer, editor.get_buffer());
        assert_eq!(new_insertion_point, editor.insertion_point());

        editor.run_edit_command(&EditCommand::Undo);
        assert_eq!(orig_buffer, editor.get_buffer());
        assert_eq!(orig_insertion_point, editor.insertion_point());
    }
}
