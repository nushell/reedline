//! Vi-standard word segmentation.
//!
//! Vi groups characters into three classes:
//! - **Keyword**: alphanumeric and underscore
//! - **Punctuation**: any non-keyword, non-whitespace char
//! - **Whitespace**
//!
//! Word boundaries occur at any class transition. This differs from reedline's
//! existing UAX #29 word boundaries (used by Emacs), which treat `foo.bar` as
//! one word; Vi sees three (`foo`, `.`, `bar`).
//!
//! `BigWord` motions (W/E/B) collapse Keyword and Punctuation: only whitespace
//! creates a boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Keyword,
    Punctuation,
    Whitespace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WordKind {
    Word,
    BigWord,
}

fn char_class(c: char, kind: WordKind) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if matches!(kind, WordKind::BigWord) || c.is_alphanumeric() || c == '_' {
        CharClass::Keyword
    } else {
        CharClass::Punctuation
    }
}

/// Vi `w`/`W` motion: jump to the start of the next class segment, skipping
/// any whitespace between segments. Returns a byte offset.
pub(super) fn word_right_start_index(buffer: &str, cursor: usize, kind: WordKind) -> usize {
    let mut iter = buffer[cursor..].char_indices().map(|(i, c)| (cursor + i, c));

    let Some((_, first)) = iter.next() else {
        return buffer.len();
    };
    let start_class = char_class(first, kind);

    let Some((boundary_pos, boundary_char)) =
        iter.find(|&(_, c)| char_class(c, kind) != start_class)
    else {
        return buffer.len();
    };

    if char_class(boundary_char, kind) != CharClass::Whitespace {
        return boundary_pos;
    }

    buffer[boundary_pos..]
        .char_indices()
        .find(|&(_, c)| char_class(c, kind) != CharClass::Whitespace)
        .map_or(buffer.len(), |(rel, _)| boundary_pos + rel)
}

/// Vi `e`/`E` motion: jump to the end of the current class segment, or to the
/// end of the next segment if already at an end. Returns a byte offset of the
/// last char in the segment.
pub(super) fn word_right_end_index(buffer: &str, cursor: usize, kind: WordKind) -> usize {
    let mut iter = buffer[cursor..].char_indices().map(|(i, c)| (cursor + i, c));

    // Always advance past the cursor's char first, so that being already at the
    // end of a word jumps to the end of the *next* one.
    if iter.next().is_none() {
        return buffer.len();
    }

    let Some((mut last_pos, start_char)) =
        iter.find(|&(_, c)| char_class(c, kind) != CharClass::Whitespace)
    else {
        return buffer.len();
    };
    let start_class = char_class(start_char, kind);

    for (pos, c) in iter {
        if char_class(c, kind) != start_class {
            break;
        }
        last_pos = pos;
    }
    last_pos
}

/// Vi `b`/`B` motion: jump to the start of the current class segment, or to
/// the start of the previous segment if already at a start. Returns a byte
/// offset.
pub(super) fn word_left_index(buffer: &str, cursor: usize, kind: WordKind) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut iter = buffer[..cursor].char_indices().rev();

    let Some((mut pos, c)) = iter.find(|&(_, c)| char_class(c, kind) != CharClass::Whitespace)
    else {
        return 0;
    };
    let target_class = char_class(c, kind);

    for (i, c) in iter {
        if char_class(c, kind) != target_class {
            break;
        }
        pos = i;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // --- small word `w` ---
    #[rstest]
    #[case("hello world", 0, 6)]
    #[case("hello world", 4, 6)]
    #[case("foo.bar", 0, 3)]
    #[case("foo.bar", 3, 4)]
    #[case("foo  bar", 0, 5)]
    #[case("hello", 0, 5)]
    #[case("", 0, 0)]
    #[case("  hello", 0, 2)]
    #[case("a_b foo", 0, 4)]
    fn small_w(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_right_start_index(buffer, cursor, WordKind::Word), expected);
    }

    // --- big word `W` ---
    #[rstest]
    #[case("foo.bar baz", 0, 8)]
    #[case("foo bar", 0, 4)]
    #[case("foo   bar", 0, 6)]
    #[case("", 0, 0)]
    fn big_w(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_right_start_index(buffer, cursor, WordKind::BigWord), expected);
    }

    // --- small word `e` ---
    #[rstest]
    #[case("hello world", 0, 4)]
    #[case("hello world", 4, 10)]
    #[case("foo.bar", 0, 2)]
    #[case("foo.bar", 2, 3)]
    #[case("foo.bar", 3, 6)]
    #[case("", 0, 0)]
    #[case("  hello", 0, 6)]
    fn small_e(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_right_end_index(buffer, cursor, WordKind::Word), expected);
    }

    // --- big word `E` ---
    #[rstest]
    #[case("foo.bar baz", 0, 6)]
    #[case("foo bar", 0, 2)]
    fn big_e(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_right_end_index(buffer, cursor, WordKind::BigWord), expected);
    }

    // --- small word `b` ---
    #[rstest]
    #[case("hello world", 6, 0)]
    #[case("hello world", 10, 6)]
    #[case("hello world", 0, 0)]
    #[case("foo.bar", 4, 3)]
    #[case("foo.bar", 3, 0)]
    #[case("   abc", 6, 3)]
    #[case("   ", 3, 0)]
    fn small_b(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_left_index(buffer, cursor, WordKind::Word), expected);
    }

    // --- big word `B` ---
    #[rstest]
    #[case("foo.bar baz", 8, 0)]
    #[case("foo.bar", 4, 0)]
    fn big_b(#[case] buffer: &str, #[case] cursor: usize, #[case] expected: usize) {
        assert_eq!(word_left_index(buffer, cursor, WordKind::BigWord), expected);
    }
}
