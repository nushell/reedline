//! Line-boundary math for `&str` buffers — the shared substrate every
//! line-aware consumer resolves through.
//!
//! Motions (`resolve_motion`), the linewise operator snap (`Editor::operate`),
//! and the [`LineBuffer`](super::LineBuffer) accessors all delegate here, so
//! line-start/line-end semantics (including CRLF handling) live in one place
//! instead of drifting across hand-rolled `find`/`rfind` copies.

/// Byte offset of the first character of the line containing `pos`.
///
/// Returns 0 for the first line.
pub(crate) fn start_of_line(buf: &str, pos: usize) -> usize {
    buf[..pos].rfind('\n').map_or(0, |i| i + 1)
    // str is guaranteed to be utf8, thus \n is safe to assume 1 byte long
}

/// Byte offset where the line containing `pos` ends, *excluding* the line
/// terminator: the position of the `\n` — or of the `\r` in a `\r\n` pair —
/// or `buf.len()` when the line is unterminated.
pub(crate) fn end_of_line(buf: &str, pos: usize) -> usize {
    match buf[pos..].find('\n') {
        None => buf.len(),
        Some(i) => {
            let newline = pos + i;
            if newline > 0 && buf.as_bytes()[newline - 1] == b'\r' {
                newline - 1
            } else {
                newline
            }
        }
    }
}

/// Byte offset of the first non-whitespace character on the line containing
/// `pos`, or `None` when the line is blank.
///
/// Bounding the search to the line is what makes the `None` reachable.
/// [`LineBuffer::line_non_blank_start_index`](super::LineBuffer::line_non_blank_start_index)
/// searches on into the buffer and settles for the terminator, so it *moves* on
/// a blank line. The bound also keeps a `\r\n`'s `\r` out of the haystack.
#[cfg(feature = "helix")]
pub(crate) fn first_non_blank(buf: &str, pos: usize) -> Option<usize> {
    let start = start_of_line(buf, pos);
    buf[start..end_of_line(buf, pos)]
        .find(|c: char| !c.is_whitespace())
        // `find` reports into the slice, not the buffer.
        .map(|offset| start + offset)
}

/// Byte offset just past the `\n` terminating the line containing `pos`, or
/// `None` when the line is unterminated (there is no line below).
pub(crate) fn start_of_next_line(buf: &str, pos: usize) -> Option<usize> {
    buf[pos..].find('\n').map(|i| pos + i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    // "ab\ncd\nef": a0 b1 \n2 c3 d4 \n5 e6 f7

    #[test]
    fn start_of_line_finds_current_line() {
        assert_eq!(start_of_line("ab\ncd\nef", 0), 0);
        assert_eq!(start_of_line("ab\ncd\nef", 2), 0); // on the \n itself
        assert_eq!(start_of_line("ab\ncd\nef", 4), 3);
        assert_eq!(start_of_line("ab\ncd\nef", 8), 6);
    }

    #[test]
    fn end_of_line_stops_at_newline_or_buffer_end() {
        assert_eq!(end_of_line("ab\ncd\nef", 0), 2);
        assert_eq!(end_of_line("ab\ncd\nef", 4), 5);
        assert_eq!(end_of_line("ab\ncd\nef", 7), 8); // unterminated last line
    }

    #[test]
    fn end_of_line_backs_over_carriage_return() {
        // CRLF terminator: the line's content ends before the \r, not the \n.
        assert_eq!(end_of_line("ab\r\ncd", 0), 2);
        assert_eq!(end_of_line("ab\r\ncd", 5), 6);
    }

    #[cfg(feature = "helix")]
    #[test]
    fn first_non_blank_finds_the_indent_end() {
        assert_eq!(first_non_blank("    foo", 0), Some(4));
        assert_eq!(first_non_blank("foo", 0), Some(0)); // unindented
        assert_eq!(first_non_blank("\t\tfoo", 0), Some(2)); // tabs count as blank
        assert_eq!(first_non_blank("    foo", 6), Some(4)); // origin past the indent
    }

    #[cfg(feature = "helix")]
    #[test]
    fn first_non_blank_reports_into_the_buffer_not_the_line() {
        // "ab\n  cd": a0 b1 \n2 ' '3 ' '4 c5 d6. `find` reports into the sliced
        // line, so only a later line catches a missing shift back.
        assert_eq!(first_non_blank("ab\n  cd", 3), Some(5));
        assert_eq!(first_non_blank("ab\n  cd", 6), Some(5));
    }

    #[cfg(feature = "helix")]
    #[test]
    fn first_non_blank_is_none_on_a_blank_line() {
        // The point of the bound: never report the *next* line's first
        // non-blank.
        assert_eq!(first_non_blank("   \nfoo", 0), None);
        assert_eq!(first_non_blank("", 0), None);
        assert_eq!(first_non_blank("   ", 1), None); // unterminated blank line
        assert_eq!(first_non_blank("foo\n\nbar", 4), None); // empty middle line
    }

    #[cfg(feature = "helix")]
    #[test]
    fn first_non_blank_ignores_a_carriage_return() {
        // `end_of_line` keeps the \r out, so a CRLF blank line reads as blank.
        assert_eq!(first_non_blank("   \r\nfoo", 0), None);
        assert_eq!(first_non_blank("  ab\r\ncd", 0), Some(2));
    }

    #[test]
    fn start_of_next_line_is_none_on_last_line() {
        assert_eq!(start_of_next_line("ab\ncd", 0), Some(3));
        assert_eq!(start_of_next_line("ab\ncd", 4), None);
        assert_eq!(start_of_next_line("ab\r\ncd", 0), Some(4));
    }
}
