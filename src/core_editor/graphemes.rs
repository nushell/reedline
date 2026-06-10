use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

/// Byte index of the next grapheme boundary at or after `pos`.
///
/// Returns `buf.len()` if there is no grapheme after `pos`.
///
/// # Panics
///
/// Panics if `pos` is not on a UTF-8 character boundary in `buf`.
pub fn next_grapheme_boundary(buf: &str, pos: usize) -> usize {
    debug_assert!(buf.is_char_boundary(pos), "pos must be a char boundary");
    buf[pos..]
        .grapheme_indices(true)
        .nth(1)
        .map(|(i, _)| pos + i)
        .unwrap_or(buf.len())
}

/// Byte index of the previous grapheme boundary before `pos`.
///
/// Returns `0` if there is no grapheme before `pos`.
///
/// # Panics
///
/// Panics if `pos` is not on a UTF-8 character boundary in `buf`.
pub fn prev_grapheme_boundary(buf: &str, pos: usize) -> usize {
    debug_assert!(buf.is_char_boundary(pos), "pos must be a char boundary");
    buf[..pos]
        .grapheme_indices(true)
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Whether `pos` sits on a grapheme boundary in `buf`.
///
/// A local check via [`GraphemeCursor`] — the cursor is given the whole buffer
/// as context, so it stays correct for context-sensitive sequences (combining
/// marks, ZWJ emoji) while only examining the bytes around `pos`, never
/// re-segmenting the buffer. Does not panic for off-boundary `pos`; it simply
/// reports `false`.
#[cfg(test)]
fn is_grapheme_boundary(buf: &str, pos: usize) -> bool {
    if pos == 0 || pos == buf.len() {
        return true;
    }
    if !buf.is_char_boundary(pos) {
        return false;
    }
    // Err is unreachable with the whole buffer as the single chunk.
    GraphemeCursor::new(pos, buf.len(), true)
        .is_boundary(buf, 0)
        .unwrap_or(false)
}

/// Snaps `pos` down to the start of the grapheme that contains it (the floor),
/// or returns `pos` unchanged when it already sits on a boundary.
///
/// Total and idempotent — a no-op on an already-aligned position, and never
/// panics (an off-boundary `pos` simply snaps to the enclosing grapheme start;
/// past-the-end clamps to `buf.len()`). Like [`is_grapheme_boundary`], a local
/// [`GraphemeCursor`] check with whole-buffer context — correct for
/// context-sensitive sequences without re-segmenting the buffer.
#[allow(dead_code)] // wired at the rest-policy commit boundary
pub(crate) fn ensure_grapheme_boundary_prev(buf: &str, pos: usize) -> usize {
    // floor to a char boundary first so the cursor seed is valid
    let mut pos = pos.min(buf.len());
    while !buf.is_char_boundary(pos) {
        pos -= 1;
    }
    let mut cursor = GraphemeCursor::new(pos, buf.len(), true);
    match cursor.is_boundary(buf, 0) {
        Ok(true) => pos,
        _ => cursor.prev_boundary(buf, 0).ok().flatten().unwrap_or(0),
    }
}

/// Snaps `pos` up to the end of the grapheme that contains it (the ceiling),
/// or returns `pos` unchanged when it already sits on a boundary.
///
/// Total and idempotent — a no-op on an already-aligned position, and never
/// panics (an off-boundary `pos` simply snaps to the enclosing grapheme end;
/// past-the-end clamps to `buf.len()`). Like [`is_grapheme_boundary`], a local
/// [`GraphemeCursor`] check with whole-buffer context — correct for
/// context-sensitive sequences without re-segmenting the buffer.
#[allow(dead_code)] // wired at the rest-policy commit boundary
pub(crate) fn ensure_grapheme_boundary_next(buf: &str, pos: usize) -> usize {
    // ceil to a char boundary first so the cursor seed is valid (`buf.len()`
    // is always one, so this terminates)
    let mut pos = pos.min(buf.len());
    while !buf.is_char_boundary(pos) {
        pos += 1;
    }
    let mut cursor = GraphemeCursor::new(pos, buf.len(), true);
    match cursor.is_boundary(buf, 0) {
        Ok(true) => pos,
        _ => cursor
            .next_boundary(buf, 0)
            .ok()
            .flatten()
            .unwrap_or(buf.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_advances_one_ascii_char() {
        assert_eq!(next_grapheme_boundary("abc", 0), 1);
    }

    #[test]
    fn next_returns_buf_len_when_at_end() {
        assert_eq!(next_grapheme_boundary("abc", 3), 3);
    }

    #[test]
    fn next_on_empty_buffer_returns_zero() {
        assert_eq!(next_grapheme_boundary("", 0), 0);
    }

    #[test]
    fn next_skips_two_byte_utf8_grapheme() {
        assert_eq!(next_grapheme_boundary("café!", 3), 5);
    }

    #[test]
    fn next_at_end_returns_buf_len() {
        let buf = "café";
        assert_eq!(next_grapheme_boundary(buf, 3), buf.len());
    }

    #[test]
    fn next_treats_combining_mark_as_single_grapheme() {
        assert_eq!(next_grapheme_boundary("e\u{0301}", 0), 3);
    }

    #[test]
    fn next_advances_one_cjk_char() {
        assert_eq!(next_grapheme_boundary("日本", 0), 3);
    }

    #[test]
    fn next_skips_zwj_emoji_sequence_as_one() {
        // family-emoji + `!`. From 0, skip the whole 18-byte sequence and land on `!`
        let prefix = "👨‍👩‍👧";
        assert_eq!(next_grapheme_boundary("👨‍👩‍👧!", 0), prefix.len());
    }

    #[test]
    fn prev_retreats_one_ascii_char() {
        assert_eq!(prev_grapheme_boundary("abc", 2), 1);
    }

    #[test]
    fn prev_at_zero_returns_zero() {
        assert_eq!(prev_grapheme_boundary("abc", 0), 0);
    }

    #[test]
    fn prev_retreats_past_two_byte_utf8_grapheme() {
        // from byte 5 (end of "café") retreat past `é` to byte 3 (its start)
        let buf = "café";
        assert_eq!(prev_grapheme_boundary(buf, buf.len()), 3);
    }

    #[test]
    fn prev_retreats_past_combining_mark() {
        // 'a' + combined 'é' (3 bytes). From end, retreat past combined grapheme to byte 1
        let buf = "ae\u{0301}";
        assert_eq!(prev_grapheme_boundary(buf, buf.len()), 1);
    }

    #[test]
    fn prev_retreats_past_zwj_emoji_sequence() {
        // 'a' + family-emoji (18 bytes). From end, retreat past the family to byte 1
        let buf = "a👨‍👩‍👧";
        assert_eq!(prev_grapheme_boundary(buf, buf.len()), 1);
    }

    #[test]
    fn next_then_prev_returns_to_origin_for_ascii() {
        let buf = "abc";
        for (pos, _) in buf.grapheme_indices(true) {
            assert_eq!(
                prev_grapheme_boundary(buf, next_grapheme_boundary(buf, pos)),
                pos,
                "round-trip failed at pos {pos}"
            );
        }
    }

    #[test]
    fn next_then_prev_returns_to_origin_for_unicode() {
        // mix ASCII, multi-byte, combining mark, and ZWJ emoji
        let buf = "a日e\u{0301}👨‍👩‍👧";
        for (pos, _) in buf.grapheme_indices(true) {
            assert_eq!(
                prev_grapheme_boundary(buf, next_grapheme_boundary(buf, pos)),
                pos,
                "round-trip failed at pos {pos}"
            );
        }
    }

    // "ae´" = 'a'(0..1) + 'e'+combining-acute(1..4). Graphemes: "a"[0,1), "é"[1,4).
    // Byte 2 is a UTF-8 char boundary (between 'e' and the combining mark) but
    // *not* a grapheme boundary — the case `ensure_*` exists to handle.
    const COMBINING: &str = "ae\u{0301}";

    #[test]
    fn is_boundary_true_at_start_end_and_grapheme_starts() {
        assert!(is_grapheme_boundary(COMBINING, 0)); // start of text
        assert!(is_grapheme_boundary(COMBINING, 1)); // start of "é"
        assert!(is_grapheme_boundary(COMBINING, COMBINING.len())); // end of text
    }

    #[test]
    fn is_boundary_false_mid_grapheme() {
        // byte 2 is inside the "é" grapheme
        assert!(!is_grapheme_boundary(COMBINING, 2));
    }

    #[test]
    fn is_boundary_true_at_zero_for_empty_buffer() {
        assert!(is_grapheme_boundary("", 0));
    }

    #[test]
    fn ensure_prev_floors_mid_grapheme_to_its_start() {
        assert_eq!(ensure_grapheme_boundary_prev(COMBINING, 2), 1);
    }

    #[test]
    fn ensure_next_ceils_mid_grapheme_to_its_end() {
        assert_eq!(ensure_grapheme_boundary_next(COMBINING, 2), 4);
    }

    #[test]
    fn ensure_is_noop_on_aligned_positions() {
        for pos in [0, 1, COMBINING.len()] {
            assert_eq!(ensure_grapheme_boundary_prev(COMBINING, pos), pos);
            assert_eq!(ensure_grapheme_boundary_next(COMBINING, pos), pos);
        }
    }

    #[test]
    fn ensure_is_idempotent() {
        // applying twice equals applying once, at every char boundary
        let buf = "a日e\u{0301}👨‍👩‍👧";
        for pos in (0..=buf.len()).filter(|&p| buf.is_char_boundary(p)) {
            let p1 = ensure_grapheme_boundary_prev(buf, pos);
            assert_eq!(ensure_grapheme_boundary_prev(buf, p1), p1, "prev at {pos}");
            let n1 = ensure_grapheme_boundary_next(buf, pos);
            assert_eq!(ensure_grapheme_boundary_next(buf, n1), n1, "next at {pos}");
        }
    }
}
