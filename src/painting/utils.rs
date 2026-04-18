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
/// If `line` fits in `terminal_columns` returns 0. A zero-width
/// `terminal_columns` can be reported by terminals mid-resize or when
/// the size is unknown; return 0 in that case rather than dividing by
/// zero (see #842).
///
/// FIXME: The zero-column guard below papers over a caller bug, it
/// doesn't solve it. `menu::list_menu::ListMenu::menu_required_lines`
/// passes `terminal_columns.saturating_sub(indicator_width + count_digits)`,
/// so on a terminal whose width is not greater than the indicator plus
/// the entry-index digits this function receives 0 and every entry is
/// reported as a single non-wrapping line. The real fix is to enforce a
/// minimum viable column budget in `menu_required_lines` (or to stop
/// subtracting the indicator width from the entry width). Tracked in
/// #842 / #428; remove this comment once the caller is fixed.
pub(crate) fn estimate_single_line_wraps(line: &str, terminal_columns: u16) -> usize {
    let terminal_columns: usize = terminal_columns.into();
    if terminal_columns == 0 {
        return 0;
    }
    let estimated_width = line_width(line);

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

    /// Narrow-terminal regression: a zero-column terminal used to panic
    /// with "attempt to divide by zero" inside the ceiling-division
    /// expression (#842). Return 0 extra wraps instead.
    #[test]
    fn estimate_single_line_wraps_zero_columns_does_not_panic() {
        assert_eq!(estimate_single_line_wraps("hello world", 0), 0);
        assert_eq!(estimate_single_line_wraps("", 0), 0);
    }

    #[rstest]
    #[case("", 80, 0)]
    #[case("hello", 80, 0)]
    #[case("abcdefghij", 5, 1)]
    #[case("abcdefghijk", 5, 2)]
    fn estimate_single_line_wraps_basic(
        #[case] line: &str,
        #[case] columns: u16,
        #[case] expected: usize,
    ) {
        assert_eq!(estimate_single_line_wraps(line, columns), expected);
    }
}
