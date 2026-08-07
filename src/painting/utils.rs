use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
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
            match &mut result {
                Cow::Borrowed(_) => {
                    // Best case 1 allocation, worst case 2 allocations.
                    // Avoid `AddAssign for Cow<str>` because its empty-LHS
                    // optimization may replace the preallocation.
                    let mut owned = String::with_capacity(input.len() + 1);
                    owned.push_str(&input[cursor..idx]);
                    owned.push_str("\r\n");
                    result = Cow::Owned(owned);
                }
                Cow::Owned(result) => {
                    result.push_str(&input[cursor..idx]);
                    result.push_str("\r\n");
                }
            }
            // Advance beyond the matched LF char (single byte)
            cursor = idx + 1;
        }
    }
    if let Cow::Owned(result) = &mut result {
        result.push_str(&input[cursor..input.len()]);
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

/// Where printing `printed` leaves the cursor, when it lands on the terminal's
/// right margin in the *deferred wrap* state.
///
/// A terminal does not move to the next row when a glyph lands in the final
/// column; it flags the cursor pending and only wraps once the next glyph
/// arrives. Saving and restoring that state is ambiguous, since terminals
/// disagree about whether DECSC/DECRC carry the flag, so the caller has to
/// place the cursor absolutely instead. Returns how many rows past the start of
/// `printed` that row is, or `None` when the run ends off the margin and
/// `RestorePosition` is already unambiguous.
///
/// Laid out one grapheme at a time rather than by dividing the run's width,
/// because the two disagree: a double-width grapheme with a single column left
/// cannot be split, so the terminal leaves that column blank and wraps early.
/// Division would report a 42-column run on a 21-column terminal as two exact
/// rows ending on the margin, when the terminal has actually wrapped twice and
/// left the cursor mid-row. That is the difference between restoring the cursor
/// and moving it somewhere it never was.
///
/// Note this is the *only* place in the painter that models wrapping this
/// precisely; [`estimate_required_lines`] and friends still divide, so row
/// reservations remain approximate for wide graphemes. Placing the cursor is
/// worth the extra pass because the error is directly visible.
pub(crate) fn deferred_wrap_row(printed: &str, terminal_columns: u16) -> Option<u16> {
    let columns: usize = terminal_columns.into();
    if columns == 0 {
        return None;
    }

    let (mut row, mut col, mut pending) = (0u16, 0usize, false);
    for grapheme in strip_ansi(printed).graphemes(true) {
        match grapheme {
            "\n" => (row, col, pending) = (row.saturating_add(1), 0, false),
            "\r" => (col, pending) = (0, false),
            _ => {
                let width = grapheme.width();
                // The wrap this grapheme's arrival was deferred until.
                if pending {
                    (row, col) = (row.saturating_add(1), 0);
                }
                // No room for the whole grapheme: the trailing column stays
                // blank and the terminal wraps before drawing it.
                if col + width > columns {
                    (row, col) = (row.saturating_add(1), 0);
                }
                col += width;
                pending = col >= columns;
            }
        }
    }

    pending.then(|| row.saturating_add(1))
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

    /// Narrow graphemes pack a row exactly, so the margin falls on every whole
    /// multiple of the width and the row is that multiple.
    #[rstest]
    #[case("", 20, None)]
    #[case("a", 20, None)]
    #[case(&"a".repeat(19), 20, None)]
    #[case(&"a".repeat(20), 20, Some(1))]
    #[case(&"a".repeat(21), 20, None)]
    #[case(&"a".repeat(40), 20, Some(2))]
    #[case(&"a".repeat(60), 20, Some(3))]
    // A hard break resets the column, so the rows before it still count.
    #[case("ab\naaaaaaaaaaaaaaaaaaaa", 20, Some(2))]
    #[case("ab\n", 20, None)]
    // Zero columns is reported by terminals mid-resize; nothing to divide by.
    #[case(&"a".repeat(20), 0, None)]
    fn deferred_wrap_row_on_narrow_graphemes(
        #[case] printed: &str,
        #[case] columns: u16,
        #[case] expected: Option<u16>,
    ) {
        assert_eq!(deferred_wrap_row(printed, columns), expected);
    }

    /// A double-width grapheme cannot straddle the margin: with one column left
    /// the terminal blanks it and wraps early, so the run occupies more rows
    /// than its width divided by the terminal's, and lands off the margin where
    /// division says it lands on one.
    ///
    /// On 21 columns each row fits ten `あ` with a column to spare, so the
    /// eleventh starts a row and only a multiple of ten ever reaches a margin
    /// (at column 20 of 21 — never, since 20 < 21).
    #[rstest]
    // 42 columns of text on a 21-column terminal: division says two exact rows
    // ending on the margin, the terminal says three rows ending at column 2.
    #[case(&"あ".repeat(21), 21, None)]
    #[case(&"あ".repeat(10), 21, None)]
    // An even width divides evenly, so wide graphemes do reach the margin.
    #[case(&"あ".repeat(10), 20, Some(1))]
    #[case(&"あ".repeat(20), 20, Some(2))]
    #[case(&"あ".repeat(9), 20, None)]
    // A narrow lead-in leaves an odd column, pushing every wide grapheme over.
    #[case(&format!("> {}", "あ".repeat(9)), 20, Some(1))]
    #[case(&format!("> {}", "あ".repeat(10)), 20, None)]
    // Narrow terminals make the blanked columns add up fast: 10 columns of text
    // across 5 columns is two exact rows by division and three by layout.
    #[case(&"あ".repeat(5), 5, None)]
    #[case(&"あ".repeat(3), 3, None)]
    // And the reverse, where the blanked columns are what carry the run *onto*
    // a margin: 9 columns of text is no multiple of 5, but the early wrap after
    // the second `あ` pushes the tail out to the end of the next row.
    #[case("あああaaa", 5, Some(2))]
    fn deferred_wrap_row_on_wide_graphemes(
        #[case] printed: &str,
        #[case] columns: u16,
        #[case] expected: Option<u16>,
    ) {
        assert_eq!(deferred_wrap_row(printed, columns), expected);
    }

    /// ANSI is stripped before layout, and a combining mark joins the grapheme
    /// it modifies rather than claiming a column of its own.
    #[rstest]
    #[case(&format!("\x1b[31m{}\x1b[0m", "a".repeat(20)), 20, Some(1))]
    #[case(&"e\u{301}".repeat(20), 20, Some(1))]
    fn deferred_wrap_row_ignores_zero_width_input(
        #[case] printed: &str,
        #[case] columns: u16,
        #[case] expected: Option<u16>,
    ) {
        assert_eq!(deferred_wrap_row(printed, columns), expected);
    }

    /// Regression: no-color rendering strips ANSI bytes before CRLF coercion,
    /// so text after the cursor can start with the raw LF that moves to the
    /// next continuation prompt. The leading replacement was lost before
    /// later newlines by `Cow<str> += ...`.
    #[test]
    fn coerce_crlf_preserves_leading_replacement_before_later_newline() {
        assert_eq!(coerce_crlf("\n::: 3\n::: 4"), "\r\n::: 3\r\n::: 4");
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
