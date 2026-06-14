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

    #[test]
    fn start_of_next_line_is_none_on_last_line() {
        assert_eq!(start_of_next_line("ab\ncd", 0), Some(3));
        assert_eq!(start_of_next_line("ab\ncd", 4), None);
        assert_eq!(start_of_next_line("ab\r\ncd", 0), Some(4));
    }
}
