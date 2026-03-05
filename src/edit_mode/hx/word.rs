//! Helix-style word boundary detection and motion target computation.
//!
//! Helix classifies characters into three categories and treats transitions
//! between classes as word boundaries:
//! - **Word**: alphanumeric + underscore
//! - **Punctuation**: everything else that isn't whitespace
//! - **Whitespace**: spaces, tabs, etc.
//!
//! "Small word" motions (w/b/e) break at any class transition.
//! "Big WORD" motions (W/B/E) only break at whitespace boundaries.
//!
//! Motion functions take an `HxRange` (anchor + head) and return a new
//! `HxRange` with both anchor and head adjusted. This keeps boundary
//! detection, position advancement, and anchor adjustment atomic.

use unicode_segmentation::UnicodeSegmentation;

use crate::core_editor::HxRange;
use crate::enums::WordMotionTarget;

/// Character classification for word boundary detection.
#[derive(Debug, PartialEq, Eq)]
enum CharClass {
    Word,
    Punctuation,
    Whitespace,
    /// Line endings are separate from whitespace (matching Helix).
    /// This ensures `\n` creates a boundary with adjacent spaces.
    Eol,
}

/// Classify a character into Word, Eol, Whitespace, or Punctuation.
fn categorize_char(ch: char) -> CharClass {
    match ch {
        '\n' => CharClass::Eol,
        ch if ch.is_alphanumeric() || ch == '_' => CharClass::Word,
        ch if ch.is_whitespace() => CharClass::Whitespace,
        _ => CharClass::Punctuation,
    }
}

fn is_word_boundary(a: char, b: char) -> bool {
    categorize_char(a) != categorize_char(b)
}

fn is_long_word_boundary(a: char, b: char) -> bool {
    match (categorize_char(a), categorize_char(b)) {
        (CharClass::Word, CharClass::Punctuation) | (CharClass::Punctuation, CharClass::Word) => {
            false
        }
        (a, b) => a != b,
    }
}

/// Precomputed byte-offset table for char-index ↔ byte-offset conversion.
///
/// Reedline uses byte offsets everywhere (insertion_point, HxRange),
/// but word motion logic iterates over `Vec<char>` with char indices
/// (matching Helix's char-index Rope API). This table bridges the two
/// coordinate systems.
///
/// Built once per word-motion invocation (O(n) where n = buffer chars).
/// Count-prefixed motions (e.g. `3w`) share a single `CharOffsets`
/// instance. For a line editor this overhead is negligible.
struct CharOffsets<'a> {
    /// The original string slice (avoids re-allocation).
    buf: &'a str,
    /// `entries[i] = (byte_offset, char)` for char index `i`.
    /// `entries[len]` has `byte_offset = buf.len()` (sentinel).
    entries: Vec<(usize, char)>,
}

impl<'a> CharOffsets<'a> {
    fn new(buf: &'a str) -> Self {
        let mut entries: Vec<(usize, char)> = buf.char_indices().collect();
        // Sentinel for one-past-end. The char value is unused — `char_at(len)`
        // should never be called in production; the sentinel only provides
        // `to_byte(len) == buf.len()`.
        entries.push((buf.len(), '\0'));
        Self { buf, entries }
    }

    /// Number of real characters (excluding sentinel).
    fn len(&self) -> usize {
        self.entries.len() - 1
    }

    /// Character at char index `i`.
    fn char_at(&self, i: usize) -> char {
        self.entries[i].1
    }

    /// Convert a char index to a byte offset. Handles index == len (returns buf.len()).
    fn to_byte(&self, char_idx: usize) -> usize {
        self.entries[char_idx].0
    }

    /// Convert a byte offset to a char index.
    ///
    /// The offset must lie on a char boundary. In debug builds this panics
    /// if the offset falls mid-character; in release builds it snaps to the
    /// nearest char index to avoid silent end-of-buffer jumps.
    fn to_char(&self, byte_offset: usize) -> usize {
        match self
            .entries
            .binary_search_by_key(&byte_offset, |&(off, _)| off)
        {
            Ok(idx) => idx,
            Err(idx) => {
                debug_assert!(
                    false,
                    "to_char called with non-boundary byte offset {byte_offset}"
                );
                idx.min(self.len())
            }
        }
    }

    /// Build an HxRange converting char indices back to byte offsets.
    fn to_byte_range(&self, anchor: usize, head: usize) -> HxRange {
        HxRange {
            anchor: self.to_byte(anchor),
            head: self.to_byte(head),
        }
    }

    /// Return the char index of the next grapheme boundary after `char_idx`.
    /// Falls back to `char_idx + 1` clamped to len.
    fn next_grapheme_char_idx(&self, char_idx: usize) -> usize {
        if char_idx >= self.len() {
            return self.len();
        }
        let byte_start = self.to_byte(char_idx);
        let slice = &self.buf[byte_start..];
        let first_grapheme_len = slice
            .grapheme_indices(true)
            .nth(1)
            .map(|(offset, _)| offset)
            .unwrap_or(slice.len());
        self.to_char(byte_start + first_grapheme_len)
    }

    /// Return the char index of the previous grapheme boundary before `char_idx`.
    /// Falls back to `char_idx - 1` clamped to 0.
    fn prev_grapheme_char_idx(&self, char_idx: usize) -> usize {
        if char_idx == 0 {
            return 0;
        }
        let byte_end = self.to_byte(char_idx);
        let slice = &self.buf[..byte_end];
        slice
            .grapheme_indices(true)
            .next_back()
            .map(|(offset, _)| self.to_char(offset))
            .unwrap_or(0)
    }
}

// ── Forward word motions ────────────────────────────────────────────────

/// Target predicates for forward word motions.
/// `NextWordStart` stops when the *next* char is non-whitespace at a boundary.
/// `NextWordEnd` stops when the *previous* char is non-whitespace at a boundary.
fn reached_word_start(boundary_fn: fn(char, char) -> bool) -> impl Fn(char, char) -> bool {
    move |prev, next| boundary_fn(prev, next) && (next == '\n' || !next.is_whitespace())
}

fn reached_word_end(boundary_fn: fn(char, char) -> bool) -> impl Fn(char, char) -> bool {
    move |prev, next| boundary_fn(prev, next) && (!prev.is_whitespace() || next == '\n')
}

/// Shared forward word motion implementation for both `w`/`W` and `e`/`E`.
///
/// Advances through the buffer, stopping when `reached(prev_ch, next_ch)`
/// returns true at a boundary. The `reached` predicate is the only
/// difference between NextWordStart and NextWordEnd motions.
fn word_right(
    buf: &str,
    range: &HxRange,
    count: usize,
    reached: impl Fn(char, char) -> bool,
) -> HxRange {
    let co = CharOffsets::new(buf);
    let len = co.len();

    let range_anchor = co.to_char(range.anchor);
    let range_head = co.to_char(range.head);

    if len == 0 || range_head >= len {
        return *range;
    }

    // Prepare range for block-cursor semantics (matching helix word_move).
    let (mut anchor, mut head) = if range_anchor < range_head {
        (co.prev_grapheme_char_idx(range_head), range_head)
    } else {
        (range_head, co.next_grapheme_char_idx(range_head).min(len))
    };

    for _ in 0..count {
        if head >= len {
            break;
        }

        let mut prev_ch: Option<char> = if head > 0 {
            Some(co.char_at(head - 1))
        } else {
            None
        };

        // Skip initial newlines.
        while head < len && co.char_at(head) == '\n' {
            prev_ch = Some(co.char_at(head));
            head += 1;
        }
        if prev_ch == Some('\n') {
            anchor = head;
        }

        let head_start = head;

        // Walk forward to target.
        while head < len {
            let next_ch = co.char_at(head);
            if prev_ch.map_or(true, |p| reached(p, next_ch)) {
                if head == head_start {
                    anchor = head;
                } else {
                    break;
                }
            }
            prev_ch = Some(next_ch);
            head += 1;
        }
    }

    co.to_byte_range(anchor, head)
}

/// `w` / `W` motion wrapper for tests.
#[cfg(test)]
fn word_right_start(buf: &str, range: &HxRange, count: usize, big: bool) -> HxRange {
    let boundary_fn = if big {
        is_long_word_boundary
    } else {
        is_word_boundary
    };
    word_right(buf, range, count, reached_word_start(boundary_fn))
}

/// `e` / `E` motion wrapper for tests.
#[cfg(test)]
fn word_right_end(buf: &str, range: &HxRange, count: usize, big: bool) -> HxRange {
    let boundary_fn = if big {
        is_long_word_boundary
    } else {
        is_word_boundary
    };
    word_right(buf, range, count, reached_word_end(boundary_fn))
}

// ── Backward word motions ───────────────────────────────────────────────

/// `b` / `B` motion: move to the start of the previous word.
///
/// Moves to the first character of the previous word.
/// Anchor is adjusted at boundary skips.
///
/// Algorithm (per iteration):
/// 1. If at boundary, step back and reset anchor.
/// 2. Skip whitespace going left.
/// 3. Go left through same-class run.
/// 4. Return final position.
fn word_left(buf: &str, range: &HxRange, count: usize, big: bool) -> HxRange {
    let co = CharOffsets::new(buf);
    let len = co.len();

    let range_anchor = co.to_char(range.anchor);
    let range_head = co.to_char(range.head);

    if len == 0 || range_head == 0 {
        return *range;
    }

    let boundary_fn: fn(char, char) -> bool = if big {
        is_long_word_boundary
    } else {
        is_word_boundary
    };

    // PrevWordStart uses the same predicate as NextWordEnd but in reverse.
    let reached = reached_word_end(boundary_fn);

    // Prepare range for block-cursor semantics (backward direction).
    let (mut anchor, mut head) = if range_anchor < range_head {
        (range_head, co.prev_grapheme_char_idx(range_head))
    } else {
        (co.next_grapheme_char_idx(range_head).min(len), range_head)
    };

    for _ in 0..count {
        if head == 0 {
            break;
        }

        // "prev_ch" in reverse iteration is the char at head (moving away from).
        let mut prev_ch: Option<char> = if head < len {
            Some(co.char_at(head))
        } else {
            None
        };

        // Skip initial newlines going backwards.
        while head > 0 && co.char_at(head - 1) == '\n' {
            head -= 1;
            prev_ch = Some(co.char_at(head));
        }
        if prev_ch == Some('\n') {
            anchor = head;
        }

        let head_start = head;

        // Walk backward to target.
        while head > 0 {
            let next_ch = co.char_at(head - 1);
            if prev_ch.map_or(true, |p| reached(p, next_ch)) {
                if head == head_start {
                    anchor = head;
                } else {
                    break;
                }
            }
            prev_ch = Some(next_ch);
            head -= 1;
        }
    }

    co.to_byte_range(anchor, head)
}

/// Dispatch a word motion by target and movement.
///
/// Routes `WordMotionTarget` to the appropriate directional function,
/// extracting the `big` flag from the target itself.
pub(crate) fn word_move(
    buf: &str,
    range: &HxRange,
    count: usize,
    target: WordMotionTarget,
) -> HxRange {
    use WordMotionTarget::*;
    let big = matches!(
        target,
        NextLongWordStart | NextLongWordEnd | PrevLongWordStart
    );
    let boundary_fn: fn(char, char) -> bool = if big {
        is_long_word_boundary
    } else {
        is_word_boundary
    };

    match target {
        NextWordStart | NextLongWordStart => {
            word_right(buf, range, count, reached_word_start(boundary_fn))
        }
        NextWordEnd | NextLongWordEnd => {
            word_right(buf, range, count, reached_word_end(boundary_fn))
        }
        PrevWordStart | PrevLongWordStart => word_left(buf, range, count, big),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test runner ─────────────────────────────────────────────────────
    //
    // Adapted from helix-core/src/movement.rs test infrastructure.
    // Internally word motions use char indices but convert to byte offsets
    // via CharOffsets. All ASCII test strings have identical char/byte values.

    /// Run a motion function against a table of (sample, scenarios).
    /// Each scenario is (count, begin_range, expected_range).
    fn run_motion_tests(
        f: fn(&str, &HxRange, usize, bool) -> HxRange,
        big: bool,
        tests: &[(&str, Vec<(usize, HxRange, HxRange)>)],
    ) {
        for (sample, scenarios) in tests {
            for (count, begin, expected) in scenarios {
                let result = f(sample, begin, *count, big);
                assert_eq!(
                    result, *expected,
                    "\n  sample:   {:?}\n  count:    {}\n  begin:    ({}, {})\n  expected: ({}, {})\n  got:      ({}, {})",
                    sample, count,
                    begin.anchor, begin.head,
                    expected.anchor, expected.head,
                    result.anchor, result.head,
                );
            }
        }
    }

    /// Shorthand for HxRange construction in test tables.
    fn r(anchor: usize, head: usize) -> HxRange {
        HxRange { anchor, head }
    }

    // ── w (next word start) ─────────────────────────────────────────────
    // Test cases from helix-core/src/movement.rs, adapted for byte offsets.

    #[test]
    fn test_w() {
        run_motion_tests(word_right_start, false, &[
            ("Basic forward motion stops at the first space",
                vec![(1, r(0, 0), r(0, 6))]),
            (" Starting from a boundary advances the anchor",
                vec![(1, r(0, 0), r(1, 10))]),
            ("Long       whitespace gap is bridged by the head",
                vec![(1, r(0, 0), r(0, 11))]),
            ("Previous anchor is irrelevant for forward motions",
                vec![(1, r(12, 0), r(0, 9))]),
            ("    Starting from whitespace moves to last space in sequence",
                vec![(1, r(0, 0), r(0, 4))]),
            ("Starting from mid-word leaves anchor at start position and moves head",
                vec![(1, r(3, 3), r(3, 9))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 0), r(0, 29))]),
            ("Jumping\n    into starting whitespace selects the spaces before 'into'",
                vec![(1, r(0, 7), r(8, 12))]),
            ("alphanumeric.!,and.?=punctuation are considered 'words' for the purposes of word motion",
                vec![
                    (1, r(0, 0), r(0, 12)),
                    (1, r(0, 12), r(12, 15)),
                    (1, r(12, 15), r(15, 18)),
                ]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 0), r(0, 6)),
                    (1, r(0, 6), r(6, 10)),
                ]),
            (".._.._ punctuation is not joined by underscores into a single block",
                vec![(1, r(0, 0), r(0, 2))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![
                    (1, r(0, 0), r(0, 8)),
                    (1, r(0, 8), r(10, 14)),
                ]),
            ("Jumping\n\n\n\n\n\n   from newlines to whitespace selects whitespace.",
                vec![(1, r(0, 9), r(13, 16))]),
            ("A failed motion does not modify the range",
                vec![(3, r(37, 41), r(37, 41))]),
            ("oh oh oh two character words!",
                vec![
                    (1, r(0, 0), r(0, 3)),
                    (1, r(0, 3), r(3, 6)),
                    (1, r(0, 2), r(1, 3)),
                ]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(0, 0), r(17, 20))]),
            ("Excessive motions are performed partially",
                vec![(999, r(0, 0), r(32, 41))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(0, 0), r(5, 5))]),
            ("\n   \n   \n Jumping through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 0), r(1, 4)),
                    (1, r(1, 4), r(5, 8)),
                ]),
        ]);
    }

    // ── W (next WORD start) ─────────────────────────────────────────────

    #[test]
    fn test_big_w() {
        run_motion_tests(word_right_start, true, &[
            ("Basic forward motion stops at the first space",
                vec![(1, r(0, 0), r(0, 6))]),
            (" Starting from a boundary advances the anchor",
                vec![(1, r(0, 0), r(1, 10))]),
            ("Long       whitespace gap is bridged by the head",
                vec![(1, r(0, 0), r(0, 11))]),
            ("Previous anchor is irrelevant for forward motions",
                vec![(1, r(12, 0), r(0, 9))]),
            ("    Starting from whitespace moves to last space in sequence",
                vec![(1, r(0, 0), r(0, 4))]),
            ("Starting from mid-word leaves anchor at start position and moves head",
                vec![(1, r(3, 3), r(3, 9))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 0), r(0, 29))]),
            ("Jumping\n    into starting whitespace selects the spaces before 'into'",
                vec![(1, r(0, 7), r(8, 12))]),
            ("alphanumeric.!,and.?=punctuation are not treated any differently than alphanumerics",
                vec![(1, r(0, 0), r(0, 33))]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 0), r(0, 6)),
                    (1, r(0, 6), r(6, 10)),
                ]),
            (".._.._ punctuation is joined by underscores into a single word, as it behaves like alphanumerics",
                vec![(1, r(0, 0), r(0, 7))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![
                    (1, r(0, 0), r(0, 8)),
                    (1, r(0, 8), r(10, 14)),
                ]),
            ("Jumping\n\n\n\n\n\n   from newlines to whitespace selects whitespace.",
                vec![(1, r(0, 9), r(13, 16))]),
            ("A failed motion does not modify the range",
                vec![(3, r(37, 41), r(37, 41))]),
            ("oh oh oh two character words!",
                vec![
                    (1, r(0, 0), r(0, 3)),
                    (1, r(0, 3), r(3, 6)),
                    (1, r(0, 1), r(0, 3)),
                ]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(0, 0), r(17, 20))]),
            ("Excessive motions are performed partially",
                vec![(999, r(0, 0), r(32, 41))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(0, 0), r(5, 5))]),
            ("\n   \n   \n Jumping through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 0), r(1, 4)),
                    (1, r(1, 4), r(5, 8)),
                ]),
        ]);
    }

    // ── b (previous word start) ─────────────────────────────────────────

    #[test]
    fn test_b() {
        run_motion_tests(word_left, false, &[
            ("Basic backward motion from the middle of a word",
                vec![(1, r(3, 3), r(4, 0))]),
            ("    Jump to start of a word preceded by whitespace",
                vec![(1, r(5, 5), r(6, 4))]),
            ("    Jump to start of line from start of word preceded by whitespace",
                vec![(1, r(4, 4), r(4, 0))]),
            ("Previous anchor is irrelevant for backward motions",
                vec![(1, r(12, 5), r(6, 0))]),
            ("    Starting from whitespace moves to first space in sequence",
                vec![(1, r(0, 4), r(4, 0))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 20), r(20, 0))]),
            ("Jumping\n    \nback through a newline selects whitespace",
                vec![(1, r(0, 13), r(12, 8))]),
            ("Jumping to start of word from the end selects the word",
                vec![(1, r(6, 7), r(7, 0))]),
            ("alphanumeric.!,and.?=punctuation are considered 'words' for the purposes of word motion",
                vec![
                    (1, r(29, 30), r(30, 21)),
                    (1, r(30, 21), r(21, 18)),
                    (1, r(21, 18), r(18, 15)),
                ]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 10), r(10, 6)),
                    (1, r(10, 6), r(6, 0)),
                ]),
            (".._.._ punctuation is not joined by underscores into a single block",
                vec![(1, r(0, 6), r(5, 3))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![(1, r(0, 10), r(8, 0))]),
            ("Jumping    \n\n\n\n\nback from within a newline group selects previous block",
                vec![(1, r(0, 13), r(11, 0))]),
            ("Failed motions do not modify the range",
                vec![(0, r(3, 0), r(3, 0))]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(18, 18), r(9, 0))]),
            ("Excessive motions are performed partially",
                vec![(999, r(40, 40), r(10, 0))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(5, 5), r(0, 0))]),
            ("   \n   \nJumping back through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 8), r(7, 4)),
                    (1, r(7, 4), r(3, 0)),
                ]),
        ]);
    }

    // ── B (previous WORD start) ─────────────────────────────────────────

    #[test]
    fn test_big_b() {
        run_motion_tests(word_left, true, &[
            ("Basic backward motion from the middle of a word",
                vec![(1, r(3, 3), r(4, 0))]),
            ("    Jump to start of a word preceded by whitespace",
                vec![(1, r(5, 5), r(6, 4))]),
            ("    Jump to start of line from start of word preceded by whitespace",
                vec![(1, r(3, 4), r(4, 0))]),
            ("Previous anchor is irrelevant for backward motions",
                vec![(1, r(12, 5), r(6, 0))]),
            ("    Starting from whitespace moves to first space in sequence",
                vec![(1, r(0, 4), r(4, 0))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 20), r(20, 0))]),
            ("Jumping\n    \nback through a newline selects whitespace",
                vec![(1, r(0, 13), r(12, 8))]),
            ("Jumping to start of word from the end selects the word",
                vec![(1, r(6, 7), r(7, 0))]),
            ("alphanumeric.!,and.?=punctuation are treated exactly the same",
                vec![(1, r(29, 30), r(30, 0))]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 10), r(10, 6)),
                    (1, r(10, 6), r(6, 0)),
                ]),
            (".._.._ punctuation is joined by underscores into a single block",
                vec![(1, r(0, 6), r(6, 0))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![(1, r(0, 10), r(8, 0))]),
            ("Jumping    \n\n\n\n\nback from within a newline group selects previous block",
                vec![(1, r(0, 13), r(11, 0))]),
            ("Failed motions do not modify the range",
                vec![(0, r(3, 0), r(3, 0))]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(19, 19), r(9, 0))]),
            ("Excessive motions are performed partially",
                vec![(999, r(40, 40), r(10, 0))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(5, 5), r(0, 0))]),
            ("   \n   \nJumping back through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 8), r(7, 4)),
                    (1, r(7, 4), r(3, 0)),
                ]),
        ]);
    }

    // ── e (next word end) ───────────────────────────────────────────────

    #[test]
    fn test_e() {
        run_motion_tests(word_right_end, false, &[
            ("Basic forward motion from the start of a word to the end of it",
                vec![(1, r(0, 0), r(0, 5))]),
            ("Basic forward motion from the end of a word to the end of the next",
                vec![(1, r(0, 5), r(5, 13))]),
            ("Basic forward motion from the middle of a word to the end of it",
                vec![(1, r(2, 2), r(2, 5))]),
            ("    Jumping to end of a word preceded by whitespace",
                vec![(1, r(0, 0), r(0, 11))]),
            ("Previous anchor is irrelevant for end of word motion",
                vec![(1, r(12, 2), r(2, 8))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 0), r(0, 28))]),
            ("Jumping\n    into starting whitespace selects up to the end of next word",
                vec![(1, r(0, 7), r(8, 16))]),
            ("alphanumeric.!,and.?=punctuation are considered 'words' for the purposes of word motion",
                vec![
                    (1, r(0, 0), r(0, 12)),
                    (1, r(0, 12), r(12, 15)),
                    (1, r(12, 15), r(15, 18)),
                ]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 0), r(0, 3)),
                    (1, r(0, 3), r(3, 9)),
                ]),
            (".._.._ punctuation is not joined by underscores into a single block",
                vec![(1, r(0, 0), r(0, 2))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![
                    (1, r(0, 0), r(0, 8)),
                    (1, r(0, 8), r(10, 13)),
                ]),
            ("Jumping\n\n\n\n\n\n   from newlines to whitespace selects to end of next word.",
                vec![(1, r(0, 8), r(13, 20))]),
            ("A failed motion does not modify the range",
                vec![(3, r(37, 41), r(37, 41))]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(0, 0), r(16, 19))]),
            ("Excessive motions are performed partially",
                vec![(999, r(0, 0), r(31, 41))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(0, 0), r(5, 5))]),
            ("\n   \n   \n Jumping through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 0), r(1, 4)),
                    (1, r(1, 4), r(5, 8)),
                ]),
        ]);
    }

    // ── E (next WORD end) ───────────────────────────────────────────────

    #[test]
    fn test_big_e() {
        run_motion_tests(word_right_end, true, &[
            ("Basic forward motion from the start of a word to the end of it",
                vec![(1, r(0, 0), r(0, 5))]),
            ("Basic forward motion from the end of a word to the end of the next",
                vec![(1, r(0, 5), r(5, 13))]),
            ("Basic forward motion from the middle of a word to the end of it",
                vec![(1, r(2, 2), r(2, 5))]),
            ("    Jumping to end of a word preceded by whitespace",
                vec![(1, r(0, 0), r(0, 11))]),
            ("Previous anchor is irrelevant for end of word motion",
                vec![(1, r(12, 2), r(2, 8))]),
            ("Identifiers_with_underscores are considered a single word",
                vec![(1, r(0, 0), r(0, 28))]),
            ("Jumping\n    into starting whitespace selects up to the end of next word",
                vec![(1, r(0, 7), r(8, 16))]),
            ("alphanumeric.!,and.?=punctuation are treated the same way",
                vec![(1, r(0, 0), r(0, 32))]),
            ("...   ... punctuation and spaces behave as expected",
                vec![
                    (1, r(0, 0), r(0, 3)),
                    (1, r(0, 3), r(3, 9)),
                ]),
            (".._.._ punctuation is joined by underscores into a single block",
                vec![(1, r(0, 0), r(0, 6))]),
            ("Newlines\n\nare bridged seamlessly.",
                vec![
                    (1, r(0, 0), r(0, 8)),
                    (1, r(0, 8), r(10, 13)),
                ]),
            ("Jumping\n\n\n\n\n\n   from newlines to whitespace selects to end of next word.",
                vec![(1, r(0, 9), r(13, 20))]),
            ("A failed motion does not modify the range",
                vec![(3, r(37, 41), r(37, 41))]),
            ("Multiple motions at once resolve correctly",
                vec![(3, r(0, 0), r(16, 19))]),
            ("Excessive motions are performed partially",
                vec![(999, r(0, 0), r(31, 41))]),
            ("",
                vec![(1, r(0, 0), r(0, 0))]),
            ("\n\n\n\n\n",
                vec![(1, r(0, 0), r(5, 5))]),
            ("\n   \n   \n Jumping through alternated space blocks and newlines selects the space blocks",
                vec![
                    (1, r(0, 0), r(1, 4)),
                    (1, r(1, 4), r(5, 8)),
                ]),
        ]);
    }

    // ── Non-ASCII / multi-byte UTF-8 ──────────────────────────────────

    #[test]
    fn test_w_multibyte() {
        // "café world" — 'é' is 2 bytes (U+00E9), so byte offsets diverge from char indices.
        // char indices: c=0 a=1 f=2 é=3 ' '=4 w=5 o=6 r=7 l=8 d=9
        // byte offsets: c=0 a=1 f=2 é=3 ' '=5 w=6 o=7 r=8 l=9 d=10
        let buf = "café world";
        assert_eq!(buf.len(), 11); // 10 chars but 11 bytes

        // w from start: "café " → anchor=0, head at 'w' (byte 6)
        // In char indices: anchor=0, head=5 → byte: anchor=0, head=6
        // But word_right_start selects "café " as anchor=0, head=5 (chars) → bytes 0, 6
        let result = word_right_start(buf, &r(0, 0), 1, false);
        assert_eq!(result.anchor, 0); // byte offset of char 0
        assert_eq!(result.head, 6); // byte offset of char 5 ('w' = byte 6)
    }

    #[test]
    fn test_b_multibyte() {
        let buf = "café world";
        // b from end (byte 10, char 9): should go to "world" start (byte 6)
        let result = word_left(buf, &r(10, 10), 1, false);
        assert_eq!(result.head, 6); // byte offset of 'w'
    }

    #[test]
    fn test_e_multibyte() {
        let buf = "café world";
        // e from start: should go to end of "café" (byte 4 = end of 'é', char 3)
        // In Helix semantics, head goes past the word end so:
        // char anchor=0, head=4 → byte anchor=0, head=5
        let result = word_right_end(buf, &r(0, 0), 1, false);
        assert_eq!(result.anchor, 0);
        assert_eq!(result.head, 5); // byte offset past 'é' (byte 3+2=5)
    }
}
