use std::borrow::Cow;
use unicode_width::UnicodeWidthStr;

/// Ensures input uses CRLF line endings.
///
/// Needed for correct output in raw mode.
/// Only replaces solitary LF with CRLF.
pub(crate) fn coerce_crlf(input: &str) -> Cow<'_, str> {
    let mut result = Cow::Borrowed(input);
    let mut cursor: usize = 0;
    for (idx, _) in input.match_indices('\n') {
        if !(idx > 0 && input.as_bytes()[idx - 1] == b'\r') {
            if let Cow::Borrowed(_) = result {
                // Best case 1 allocation, worst case 2 allocations
                let mut owned = String::with_capacity(input.len() + 1);
                // Optimization to avoid the `AddAssign for Cow<str>`
                // optimization for `Cow<str>.is_empty` that would replace the
                // preallocation
                owned.push_str(&input[cursor..idx]);
                result = Cow::Owned(owned);
            } else {
                result += &input[cursor..idx];
            }
            result += "\r\n";
            // Advance beyond the matched LF char (single byte)
            cursor = idx + 1;
        }
    }
    if let Cow::Owned(_) = result {
        result += &input[cursor..input.len()];
    }
    result
}

/// Returns string with the ANSI escape codes removed
///
/// If parsing fails silently returns the input string
pub(crate) fn strip_ansi(string: &str) -> String {
    String::from_utf8(strip_ansi_escapes::strip(string))
        .map_err(|_| ())
        .unwrap_or_else(|_| string.to_owned())
}

pub(crate) fn estimate_required_lines(input: &str, screen_width: u16) -> usize {
    input.lines().fold(0, |acc, line| {
        let wrap = estimate_single_line_wraps(line, screen_width);

        acc + 1 + wrap
    })
}

/// Reports the additional lines needed due to wrapping for the given line.
///
/// Does not account for any potential line breaks in `line`
///
/// If `line` fits in `terminal_columns` returns 0
pub(crate) fn estimate_single_line_wraps(line: &str, terminal_columns: u16) -> usize {
    let estimated_width = line_width(line);
    let terminal_columns: usize = terminal_columns.into();

    // integer ceiling rounding division for positive divisors
    let estimated_line_count = (estimated_width + terminal_columns - 1) / terminal_columns;

    // Any wrapping will add to our overall line count
    estimated_line_count.saturating_sub(1)
}

/// Compute the line width for ANSI escaped text
pub(crate) fn line_width(line: &str) -> usize {
    strip_ansi(line).width()
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case("sentence\nsentence", "sentence\r\nsentence")]
    #[case("sentence\r\nsentence", "sentence\r\nsentence")]
    #[case("sentence\nsentence\n", "sentence\r\nsentence\r\n")]
    #[case("😇\nsentence", "😇\r\nsentence")]
    #[case("sentence\n😇", "sentence\r\n😇")]
    #[case("\n", "\r\n")]
    #[case("", "")]
    fn test_coerce_crlf(#[case] input: &str, #[case] expected: &str) {
        let result = coerce_crlf(input);

        assert_eq!(result, expected);

        assert!(
            input != expected || matches!(result, Cow::Borrowed(_)),
            "Unnecessary allocation"
        )
    }
}
