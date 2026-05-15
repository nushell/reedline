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
//!
//! Iteration walks grapheme clusters (via `unicode_segmentation::grapheme_indices`)
//! so multi-codepoint sequences (combining marks, ZWJ emoji) are treated as one
//! unit, matching the rest of `core_editor`. Each cluster is classified by its
//! first scalar.

use unicode_segmentation::UnicodeSegmentation;

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

fn class_of(grapheme: &str, kind: WordKind) -> CharClass {
    let c = grapheme
        .chars()
        .next()
        .expect("grapheme cluster is non-empty");
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
    let mut iter = buffer[cursor..]
        .grapheme_indices(true)
        .map(|(i, g)| (cursor + i, g));

    let Some((_, first)) = iter.next() else {
        return buffer.len();
    };
    let start_class = class_of(first, kind);

    let Some((boundary_pos, boundary_g)) =
        iter.find(|&(_, g)| class_of(g, kind) != start_class)
    else {
        return buffer.len();
    };

    if class_of(boundary_g, kind) != CharClass::Whitespace {
        return boundary_pos;
    }

    iter.find(|&(_, g)| class_of(g, kind) != CharClass::Whitespace)
        .map_or(buffer.len(), |(pos, _)| pos)
}

/// Vi `e`/`E` motion: jump to the end of the current class segment, or to the
/// end of the next segment if already at an end. Returns the byte offset of
/// the last grapheme in the segment.
pub(super) fn word_right_end_index(buffer: &str, cursor: usize, kind: WordKind) -> usize {
    let mut iter = buffer[cursor..]
        .grapheme_indices(true)
        .map(|(i, g)| (cursor + i, g));

    // Always advance past the cursor's grapheme first, so that being already at
    // the end of a word jumps to the end of the *next* one.
    if iter.next().is_none() {
        return buffer.len();
    }

    let Some((mut last_pos, start_g)) =
        iter.find(|&(_, g)| class_of(g, kind) != CharClass::Whitespace)
    else {
        return buffer.len();
    };
    let start_class = class_of(start_g, kind);

    for (pos, g) in iter {
        if class_of(g, kind) != start_class {
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

    let mut iter = buffer[..cursor].grapheme_indices(true).rev();

    let Some((mut pos, g)) = iter.find(|&(_, g)| class_of(g, kind) != CharClass::Whitespace)
    else {
        return 0;
    };
    let target_class = class_of(g, kind);

    for (i, g) in iter {
        if class_of(g, kind) != target_class {
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
    // Unicode: precomposed multi-byte, combining mark, ZWJ emoji
    #[case("café foo", 0, 6)]
    #[case("e\u{0301} foo", 0, 4)]
    #[case("👨‍👩‍👧 foo", 0, 19)]
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
    // Unicode: lands on the byte offset of the last grapheme's start
    #[case("café foo", 0, 3)]
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
    // Unicode: backwards iteration must not split a grapheme cluster
    #[case("café foo", 9, 6)]
    #[case("café", 5, 0)]
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
