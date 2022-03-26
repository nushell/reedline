//! Collection of common functions that can be used to create menus
use crate::Suggestion;

/// Index result obtained from parsing a string with an index marker
/// For example, the next string:
///     "this is an example :10"
///
/// Contains an index marker :10. This marker indicates that the user
/// may want to select the 10th element from a list
#[derive(Debug, PartialEq)]
pub struct ParseResult<'buffer> {
    /// Text before the marker
    pub remainder: &'buffer str,
    /// Parsed value from the marker
    pub index: Option<usize>,
    /// Marker representation as string
    pub marker: Option<&'buffer str>,
    /// Direction of the search based on the marker
    pub direction: IndexDirection,
}

/// Direction of the index found in the string
#[derive(Debug, PartialEq)]
pub enum IndexDirection {
    /// Forward index
    Forward,
    /// Backward index
    Backward,
}

/// Splits a string that contains a marker character
///
/// ## Example usage
/// ```
/// use reedline::menu_functions::{parse_selection_char, IndexDirection, ParseResult};
///
/// let parsed = parse_selection_char("this is an example!10", '!');
///
/// assert_eq!(
///     parsed,
///     ParseResult {
///         remainder: "this is an example",
///         index: Some(10),
///         marker: Some("!10"),
///         direction: IndexDirection::Forward
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
            direction: IndexDirection::Forward,
        };
    }

    let mut input = buffer.chars().peekable();

    let mut index = 0;
    let mut direction = IndexDirection::Forward;
    while let Some(char) = input.next() {
        if char == marker {
            match input.peek() {
                Some(&x) if x == marker => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + 2]),
                        direction: IndexDirection::Backward,
                    }
                }
                Some(&x) if x.is_ascii_digit() || x == '-' => {
                    let mut count: usize = 0;
                    let mut size: usize = 1;
                    while let Some(&c) = input.peek() {
                        if c == '-' {
                            let _ = input.next();
                            size += 1;
                            direction = IndexDirection::Backward;
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
                                direction,
                            };
                        }
                    }
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(count),
                        marker: Some(&buffer[index..index + size]),
                        direction,
                    };
                }
                None => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..buffer.len()]),
                        direction,
                    }
                }
                _ => {
                    index += 1;
                    continue;
                }
            }
        }
        index += 1;
    }

    ParseResult {
        remainder: buffer,
        index: None,
        marker: None,
        direction,
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
                    .chars()
                    .zip(suggestion.value.chars())
                    .position(|(mut lhs, mut rhs)| {
                        lhs.make_ascii_lowercase();
                        rhs.make_ascii_lowercase();

                        lhs != rhs
                    })
                    .map(|new_index| match index {
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

    let old_chars = old_string.chars().collect::<Vec<char>>();

    let (_, start, end) = new_string.chars().enumerate().fold(
        (0, None, None),
        |(old_index, start, end), (index, c)| {
            let equal = if start.is_some() {
                if (old_string.len() - old_index) == (new_string.len() - index) {
                    let new_iter = new_string.chars().skip(index);
                    let old_iter = old_string.chars().skip(old_index);

                    new_iter.zip(old_iter).all(|(new, old)| new == old)
                } else {
                    false
                }
            } else {
                c == old_chars[old_index]
            };

            if equal {
                let old_index = (old_index + 1).min(old_string.len() - 1);

                let end = match (start, end) {
                    (Some(_), Some(_)) => end,
                    (Some(_), None) => Some(index),
                    _ => None,
                };

                (old_index, start, end)
            } else {
                let start = match start {
                    Some(_) => start,
                    None => Some(index),
                };

                (old_index, start, end)
            }
        },
    );

    match (start, end) {
        (Some(start), Some(end)) => (start, &new_string[start..end]),
        (Some(start), None) => (start, &new_string[start..new_string.len()]),
        (None, None) => (new_string.len(), ""),
        (None, Some(_)) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_row_test() {
        let input = "search:6";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(6));
        assert_eq!(res.marker, Some(":6"));
    }

    #[test]
    fn parse_double_char() {
        let input = "search!!";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some("!!"));
        assert!(matches!(res.direction, IndexDirection::Backward));
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
        assert!(matches!(res.direction, IndexDirection::Backward));
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
        let new_string = "let b = ñ ";
        let old_string = "let a =";

        let res = string_difference(new_string, old_string);
        assert_eq!(res, (4, "b = ñ "));
    }
}
