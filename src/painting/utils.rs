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
        if !(idx > 0 && input.as_bytes().get(idx - 1) == Some(&b'\r')) {
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
/// Callers pre-split, so `line` is not expected to contain a line break;
/// [`wrap_position`] would count one as a row.
///
/// If `line` fits in `terminal_columns` returns 0. A zero-width
/// `terminal_columns` can be reported by terminals mid-resize or when the
/// size is unknown; there is no layout to report, so [`wrap_position`]
/// answers `None` and this reports 0 (see #842).
///
/// Rows the line *wraps onto*, not rows the cursor reaches: a line filling
/// the width exactly wraps onto none and leaves the cursor pending on the
/// next. Only the callers placing a cursor count that row.
///
/// FIXME: the zero-column guard papers over a caller bug, it
/// doesn't solve it. `menu::list_menu::ListMenu::menu_required_lines`
/// passes `terminal_columns.saturating_sub(indicator_width + count_digits)`,
/// so on a terminal whose width is not greater than the indicator plus
/// the entry-index digits this function receives 0 and every entry is
/// reported as a single non-wrapping line. The real fix is to enforce a
/// minimum viable column budget in `menu_required_lines` (or to stop
/// subtracting the indicator width from the entry width). Tracked in
/// #842 / #428; remove this comment once the caller is fixed.
pub(crate) fn estimate_single_line_wraps(line: &str, terminal_columns: u16) -> usize {
    let Some((_, row)) = wrap_position([line], terminal_columns) else {
        return 0;
    };
    row as usize
}

/// Compute the line width for ANSI escaped text
pub(crate) fn line_width(line: &str) -> usize {
    strip_ansi(line).width()
}

/// Which row to move to when printing `pieces` lands the cursor on the
/// terminal's right margin, in the *deferred wrap* state.
///
/// A terminal does not move to the next row when a glyph lands in the final
/// column; it flags the cursor pending and only wraps once the next glyph
/// arrives. Terminals disagree about whether DECSC/DECRC carry that flag, so a
/// save taken there restores to either side of the margin and the caller has to
/// place the cursor absolutely instead. Returns how many rows past the start of
/// the run that row is, or `None` off the margin, where restoring is already
/// unambiguous. `pieces` are laid out end to end, since the walk is a fold and
/// never looks backwards.
pub(crate) fn deferred_wrap_row<'a>(
    pieces: impl IntoIterator<Item = &'a str>,
    terminal_columns: u16,
) -> Option<u16> {
    let end = wrap_position(pieces, terminal_columns)?;
    (end.0 >= terminal_columns).then(|| resolve_wrap(end, terminal_columns).1)
}

/// Where the cursor rests once a [`wrap_position`] landing is resolved: a run
/// ending on the margin left the wrap pending, and that belongs to the home
/// column of the next row (#1141), not to the column it filled.
///
/// The single owner of that rule, since every caller placing a cursor needs it
/// and one of them getting the saturation wrong is a panic, not a drawing bug.
pub(crate) fn resolve_wrap((col, row): (u16, u16), terminal_columns: u16) -> (u16, u16) {
    if col >= terminal_columns {
        (0, row.saturating_add(1))
    } else {
        (col, row)
    }
}

/// Where printing `pieces` leaves the cursor, walked a grapheme at a time
/// rather than divided out of the run's width. A double-width grapheme with
/// one column left cannot be split, so the terminal blanks that column and
/// wraps early: division reads a 42-column run on a 21-column terminal as two
/// exact rows ending on the margin, when the terminal needs three.
///
/// Returns `(column, rows past the start of the run)`. The column may equal
/// `terminal_columns`: the *deferred wrap* state, where the run has filled the
/// row but nothing has arrived to push it over. Resolving that is the caller's
/// job: [`resolve_wrap`] for the cursor-placing ones, and the estimators do
/// not count it at all.
pub(crate) fn wrap_position<'a>(
    pieces: impl IntoIterator<Item = &'a str>,
    terminal_columns: u16,
) -> Option<(u16, u16)> {
    let columns: usize = terminal_columns.into();
    if columns == 0 {
        return None;
    }
    let (mut row, mut col) = (0u16, 0usize);
    for piece in pieces {
        for grapheme in strip_ansi(piece).graphemes(true) {
            match grapheme {
                "\n" => (row, col) = (row.saturating_add(1), 0),
                "\r" => col = 0,
                _ => {
                    let width = grapheme.width();
                    // The wrap this grapheme's arrival was deferred until.
                    // Both checks can fire for one grapheme, so a single `||`
                    // loses a row: a zero-width one trips only this.
                    if col >= columns {
                        (row, col) = (row.saturating_add(1), 0);
                    }
                    // No room for the whole grapheme: the trailing column stays
                    // blank and the terminal wraps before drawing it.
                    if col + width > columns {
                        (row, col) = (row.saturating_add(1), 0);
                    }
                    col += width;
                }
            }
        }
    }
    Some((col as u16, row))
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
        assert_eq!(deferred_wrap_row([printed], columns), expected);
    }

    /// Wide graphemes only diverge from division when the width leaves an odd
    /// column for one to straddle. The even-width cases pin down the agreement,
    /// the rest are what a revert to division would break.
    #[rstest]
    // 42 columns on a 21-column terminal: division reads two exact rows ending
    // on the margin, the terminal needs three and ends at column 2.
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
        assert_eq!(deferred_wrap_row([printed], columns), expected);
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
        assert_eq!(deferred_wrap_row([printed], columns), expected);
    }

    /// The column [`deferred_wrap_row`] throws away: it only asks whether the
    /// run reached the margin, so the cases above prove one bit of this tuple.
    /// The two `"hello"` cases are that tuple at two widths, since a column is
    /// only a margin relative to what it is compared against.
    #[rstest]
    #[case("hello", 5, Some((5, 0)))]
    #[case("hello", 6, Some((5, 0)))]
    // Wide graphemes straddling an odd margin, which division reads as an
    // exact row and gets both axes wrong on.
    #[case("日本語日本語", 11, Some((2, 1)))]
    #[case("日本語日本語日本語日本", 11, Some((2, 2)))]
    #[case("日a日a日a日a", 7, Some((6, 1)))]
    #[case("aaa日本語", 7, Some((2, 1)))]
    #[case("hello world", 5, Some((1, 2)))]
    // A hard break lands on column 0 of the next row without touching the
    // margin, so the column alone cannot stand in for the deferred wrap.
    #[case("ab\n", 5, Some((0, 1)))]
    #[case("ab\nc", 5, Some((1, 1)))]
    // A zero-width grapheme at the margin, which the straddle check cannot
    // see since adding nothing never exceeds the width.
    #[case(&format!("{}\u{200b}", "a".repeat(20)), 20, Some((0, 1)))]
    // Wider than the whole terminal, so the column ends past the margin and
    // the next grapheme spends a row on each check.
    #[case("日日", 1, Some((2, 3)))]
    // No layout exists without columns to lay out in.
    #[case(&"a".repeat(20), 0, None)]
    fn wrap_position_reports_the_landing_column(
        #[case] printed: &str,
        #[case] columns: u16,
        #[case] expected: Option<(u16, u16)>,
    ) {
        assert_eq!(wrap_position([printed], columns), expected);
    }

    /// A margin landing on the last row a `u16` can name. `wrap_position`
    /// saturates `row`, so the resolver has to as well, or it panics in debug
    /// on a buffer long enough to reach it.
    #[test]
    fn resolve_wrap_saturates_at_the_last_row() {
        assert_eq!(resolve_wrap((20, u16::MAX), 20), (0, u16::MAX));
        assert_eq!(resolve_wrap((20, 3), 20), (0, 4));
        // Off the margin the landing passes through untouched.
        assert_eq!(resolve_wrap((5, u16::MAX), 20), (5, u16::MAX));
    }

    /// `pieces` are laid end to end, not measured one at a time: `"ab"` and
    /// `"cd"` each fit three columns alone, and together they do not.
    #[test]
    fn wrap_position_lays_pieces_end_to_end() {
        assert_eq!(wrap_position(["ab", "cd"], 3), Some((1, 1)));
        assert_eq!(wrap_position(["abcd"], 3), Some((1, 1)));
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
    // Wide graphemes straddle the margin instead of packing it, so the text
    // takes a row division cannot see: 22 columns across 11 is two exact rows
    // by division and three by layout, 10 across 5 is two and three.
    #[case("日本語日本語日本語日本", 11, 2)]
    #[case(&"あ".repeat(5), 5, 2)]
    fn estimate_single_line_wraps_basic(
        #[case] line: &str,
        #[case] columns: u16,
        #[case] expected: usize,
    ) {
        assert_eq!(estimate_single_line_wraps(line, columns), expected);
    }
}
