//! Character classification for word-boundary detection — the shared substrate
//! for vi, emacs, and helix word motions.
//!
//! Every "word" notion in the editor is built on one classifier: each character
//! is sorted into a [`CharClass`], and a word boundary is a *transition* between
//! classes. The flavors differ only in which transitions count:
//! - **small word** (`w`/`b`/`e`): any class change is a boundary.
//! - **big WORD** (`W`/`B`/`E`): only whitespace/EOL transitions count, so a run
//!   of `Word` and `Punctuation` together is one WORD.
//!
//! Modes pick a flavor; the resolver (`locate_word`) scans with the matching
//! predicate. Keeping the classifier here — mode-agnostic and tested in isolation
//! — means vi-word, vi-WORD, emacs-word, and helix-word are thin variations over
//! one definition rather than eight ad-hoc functions.

use crate::core_editor::graphemes::prev_grapheme_boundary;
use crate::enums::{WordEdge, WordKind};

/// Classification of a character for word-boundary detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CharClass {
    /// Alphanumeric or `_` — the characters that make up a "word".
    Word,
    /// Anything else that isn't whitespace or a line ending.
    Punctuation,
    /// Spaces, tabs, etc.
    Whitespace,
    /// Line endings, kept separate from whitespace so a `\n` always forms a
    /// boundary with adjacent spaces (a word motion never glides across lines).
    Eol,
}

/// Sort `ch` into a [`CharClass`].
pub(crate) fn categorize_char(ch: char) -> CharClass {
    match ch {
        '\n' => CharClass::Eol,
        ch if ch.is_alphanumeric() || ch == '_' => CharClass::Word,
        ch if ch.is_whitespace() => CharClass::Whitespace,
        _ => CharClass::Punctuation,
    }
}

/// `true` if a *small word* boundary lies between `a` and `b` — any class change.
pub(crate) fn is_word_boundary(a: char, b: char) -> bool {
    categorize_char(a) != categorize_char(b)
}

/// `true` if a *big WORD* boundary lies between `a` and `b` — a class change,
/// except `Word`↔`Punctuation`, which stay fused into one WORD.
pub(crate) fn is_long_word_boundary(a: char, b: char) -> bool {
    match (categorize_char(a), categorize_char(b)) {
        (CharClass::Word, CharClass::Punctuation) | (CharClass::Punctuation, CharClass::Word) => {
            false
        }
        (a, b) => a != b,
    }
}

/// Byte offset of the word boundary reached from `origin`, scanning `forward`
/// (or backward), using `kind`'s boundary predicate and landing on `edge`.
///
/// This is the single resolver the 8 ad-hoc `LineBuffer::*_index` functions
/// collapse into. The `(forward, edge)` pairs map to vi motions:
/// - `(true,  Start)` → `w` / `W`   (next word's first char)
/// - `(true,  End)`   → `e` / `E`   (next word's last char, inclusive)
/// - `(false, Start)` → `b` / `B`   (previous word's first char)
pub(crate) fn locate_word(
    buf: &str,
    origin: usize,
    kind: WordKind,
    edge: WordEdge,
    forward: bool,
) -> usize {
    // The only thing `kind` changes is which transitions count as a boundary.
    let is_boundary: fn(char, char) -> bool = match kind {
        WordKind::Small => is_word_boundary,
        WordKind::Big => is_long_word_boundary,
    };

    // Is `ch` (with neighbors `before`/`after`, `None` at the buffer edges) the
    // `edge` of a word? A word excludes whitespace/EOL, so its `Start` is a
    // non-whitespace char with a boundary on its left (or the buffer start),
    // and its `End` one with a boundary on its right (or the buffer end).
    let is_target = |before: Option<char>, ch: char, after: Option<char>| -> bool {
        // A word excludes whitespace and line endings — use the module's own
        // classifier rather than a bare `is_whitespace` so `\n` (an `Eol`) is
        // handled by the same definition the boundary checks trust.
        if matches!(categorize_char(ch), CharClass::Whitespace | CharClass::Eol) {
            return false;
        }
        match edge {
            WordEdge::Start => before.map_or(true, |b| is_boundary(b, ch)),
            WordEdge::End => after.map_or(true, |a| is_boundary(ch, a)),
        }
    };

    // Scan outward from the origin without materializing the buffer: each
    // candidate's outer neighbor is read through `peek()`, its inner neighbor
    // carried from the previous step (seeded with the char on the origin's
    // other side).
    if forward {
        // first target strictly after origin
        let mut before = buf[..origin].chars().next_back();
        let mut iter = buf[origin..]
            .char_indices()
            .map(|(i, c)| (origin + i, c))
            .peekable();
        while let Some((byte, ch)) = iter.next() {
            let after = iter.peek().map(|&(_, c)| c);
            if byte > origin && is_target(before, ch, after) {
                return byte;
            }
            before = Some(ch);
        }
        // none: `w` runs to the buffer end; `e` rests on the last grapheme
        match edge {
            WordEdge::Start => buf.len(),
            WordEdge::End => prev_grapheme_boundary(buf, buf.len()),
        }
    } else {
        // nearest target strictly before origin
        let mut after = buf[origin..].chars().next();
        let mut iter = buf[..origin].char_indices().rev().peekable();
        while let Some((byte, ch)) = iter.next() {
            let before = iter.peek().map(|&(_, c)| c);
            if is_target(before, ch, after) {
                return byte;
            }
            after = Some(ch);
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorize_each_class() {
        assert_eq!(categorize_char('a'), CharClass::Word);
        assert_eq!(categorize_char('Z'), CharClass::Word);
        assert_eq!(categorize_char('7'), CharClass::Word);
        assert_eq!(categorize_char('_'), CharClass::Word);
        assert_eq!(categorize_char('é'), CharClass::Word); // unicode alphanumeric
        assert_eq!(categorize_char(' '), CharClass::Whitespace);
        assert_eq!(categorize_char('\t'), CharClass::Whitespace);
        assert_eq!(categorize_char('\n'), CharClass::Eol);
        assert_eq!(categorize_char('.'), CharClass::Punctuation);
        assert_eq!(categorize_char('-'), CharClass::Punctuation);
    }

    #[test]
    fn small_word_boundary_is_any_class_change() {
        assert!(is_word_boundary('a', '.')); // Word → Punctuation
        assert!(is_word_boundary('.', 'a')); // Punctuation → Word
        assert!(is_word_boundary('a', ' ')); // Word → Whitespace
        assert!(is_word_boundary(' ', '\n')); // Whitespace → Eol
        assert!(!is_word_boundary('a', 'b')); // both Word
        assert!(!is_word_boundary('.', ',')); // both Punctuation
    }

    #[test]
    fn big_word_boundary_fuses_word_and_punctuation() {
        // Word↔Punctuation is NOT a big-WORD boundary (e.g. `foo.bar` is one WORD)
        assert!(!is_long_word_boundary('o', '.'));
        assert!(!is_long_word_boundary('.', 'b'));
        // but whitespace/eol transitions still are
        assert!(is_long_word_boundary('a', ' '));
        assert!(is_long_word_boundary('.', ' '));
        assert!(is_long_word_boundary(' ', '\n'));
        assert!(!is_long_word_boundary('a', 'b')); // same class, no boundary
    }
}
