use std::cmp::Ordering;

use crate::core_editor::{
    graphemes::{
        ensure_grapheme_boundary_next, ensure_grapheme_boundary_prev, next_grapheme_boundary,
        prev_grapheme_boundary,
    },
    Cursor,
};

/// Where the cursor's head may *rest* after an edit, per editor paradigm.
///
/// Applied at the single commit boundary by [`commit`], layered on top of the
/// universal coherence pass ([`recohere`]). Each edit mode maps to one variant;
/// the core never inspects the mode, only the resulting policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestPolicy {
    /// The head may rest anywhere between graphemes, including past the last one
    /// (at `buf.len()`). Emacs / Vi insert — the ordinary text caret.
    Between,
    /// The head may not rest past the last grapheme: a head at the end is pulled
    /// back onto the final grapheme. Vi normal mode — the cursor invariant
    /// enforced structurally here instead of by a maintenance command.
    OnGrapheme,
    /// The resting cursor always covers exactly one grapheme: a point is widened
    /// onto the grapheme to its right, or — at the buffer end, where there is
    /// none — onto the grapheme to its left. Mirrors Helix's `Range::min_width_1`.
    /// Vi normal / Helix. No producer until those modes are wired, so it is
    /// intentionally unconstructed for now.
    #[allow(dead_code)]
    Block,
}

/// Make a cursor *coherent* for `buf`: clamp both ends into `[0, buf.len()]` and
/// snap them to grapheme boundaries, expanding outward so whole graphemes stay
/// covered — the low end floors, the high end ceils, and a point floors while
/// staying a point.
///
/// Universal across edit modes and idempotent, so it is safe to run after any
/// edit. The mode-specific resting rule is layered on top by [`commit`].
fn recohere(buf: &str, c: Cursor) -> Cursor {
    let len = buf.len();

    let head = c.head().min(len);
    let anchor = c.anchor().min(len);

    let (anchor, head) = match anchor.cmp(&head) {
        // point: both floor to keep it a point
        Ordering::Equal => {
            let pos = ensure_grapheme_boundary_prev(buf, anchor);
            (pos, pos)
        }
        // forward: anchor (low) floors, head (high) ceils
        Ordering::Less => (
            ensure_grapheme_boundary_prev(buf, anchor),
            ensure_grapheme_boundary_next(buf, head),
        ),
        // backward: anchor (high) ceils, head (low) floors
        Ordering::Greater => (
            ensure_grapheme_boundary_next(buf, anchor),
            ensure_grapheme_boundary_prev(buf, head),
        ),
    };

    Cursor::new(anchor, head)
}

/// Normalize a cursor at the one commit boundary: first [`recohere`] it (universal
/// coherence), then apply the mode's [`RestPolicy`] resting rule.
///
/// Total and idempotent — `commit(buf, commit(buf, c, p), p) == commit(buf, c, p)`
/// for every input — so it can run after every command without the cursor drifting.
pub(crate) fn commit(buf: &str, c: Cursor, policy: RestPolicy) -> Cursor {
    let c = recohere(buf, c);
    let len = buf.len();
    match policy {
        RestPolicy::Between => c,
        RestPolicy::OnGrapheme => {
            if c.head() == len && len > 0 {
                let prev = prev_grapheme_boundary(buf, c.head());
                if c.is_empty() {
                    Cursor::point(prev)
                } else {
                    c.move_head(prev)
                }
            } else {
                c
            }
        }
        RestPolicy::Block => {
            // A block cursor always covers exactly one grapheme. Only a resting
            // *point* needs adjusting; an existing selection is already a range.
            if c.is_empty() {
                let head = c.head();
                let next = next_grapheme_boundary(buf, head);
                if next > head {
                    // widen forward onto the grapheme to the right: [head, next)
                    c.move_head(next)
                } else if head > 0 {
                    // at the buffer end there's nothing to the right, so cover the
                    // last grapheme instead: [prev, head)
                    Cursor::new(prev_grapheme_boundary(buf, head), head)
                } else {
                    // empty buffer: nothing to cover
                    c
                }
            } else {
                c
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Buffers used across tests:
    //   "hello"   — ASCII, len 5, grapheme starts 0,1,2,3,4
    //   "caf\u{e9}" — precomposed é (2 bytes), len 5, last grapheme starts at 3
    //   "ae\u{0301}" — e + combining acute, len 4; "é" grapheme is [1,4),
    //                  so byte 2 is a char boundary but *not* a grapheme boundary
    const MIXED: &str = "a日e\u{0301}👨‍👩‍👧"; // ASCII + CJK + combining + ZWJ emoji

    // --- recohere ------------------------------------------------------------

    #[test]
    fn recohere_is_identity_on_aligned_cursors() {
        let b = "hello";
        assert_eq!(recohere(b, Cursor::new(1, 3)), Cursor::new(1, 3));
        assert_eq!(recohere(b, Cursor::new(3, 1)), Cursor::new(3, 1));
        assert_eq!(recohere(b, Cursor::point(2)), Cursor::point(2));
    }

    #[test]
    fn recohere_clamps_past_end_to_point_at_len() {
        assert_eq!(recohere("hello", Cursor::new(10, 12)), Cursor::point(5));
    }

    #[test]
    fn recohere_floors_mid_grapheme_point() {
        // point at byte 2 sits inside "é" [1,4) → floors to its start, 1
        assert_eq!(recohere("ae\u{0301}", Cursor::point(2)), Cursor::point(1));
    }

    #[test]
    fn recohere_expands_selection_outward() {
        // anchor 0 (low) floors to 0, head 2 (high, mid-grapheme) ceils to 4
        assert_eq!(recohere("ae\u{0301}", Cursor::new(0, 2)), Cursor::new(0, 4));
    }

    #[test]
    fn recohere_is_idempotent() {
        for a in (0..=MIXED.len()).filter(|&i| MIXED.is_char_boundary(i)) {
            for h in (0..=MIXED.len()).filter(|&i| MIXED.is_char_boundary(i)) {
                let once = recohere(MIXED, Cursor::new(a, h));
                assert_eq!(recohere(MIXED, once), once, "anchor={a} head={h}");
            }
        }
    }

    // --- commit: Between -----------------------------------------------------

    #[test]
    fn between_leaves_head_past_last_grapheme() {
        assert_eq!(
            commit("hello", Cursor::point(5), RestPolicy::Between),
            Cursor::point(5)
        );
    }

    #[test]
    fn between_only_recoheres() {
        // mid-grapheme point still floors (that's recohere); the policy adds nothing
        assert_eq!(
            commit("ae\u{0301}", Cursor::point(2), RestPolicy::Between),
            Cursor::point(1)
        );
    }

    // --- commit: OnGrapheme --------------------------------------------------

    #[test]
    fn on_grapheme_pulls_point_back_from_end() {
        assert_eq!(
            commit("hello", Cursor::point(5), RestPolicy::OnGrapheme),
            Cursor::point(4)
        );
    }

    #[test]
    fn on_grapheme_pulls_back_over_multibyte_grapheme() {
        // "café": last grapheme é starts at byte 3
        assert_eq!(
            commit("caf\u{e9}", Cursor::point(5), RestPolicy::OnGrapheme),
            Cursor::point(3)
        );
    }

    #[test]
    fn on_grapheme_leaves_midbuffer_point() {
        assert_eq!(
            commit("hello", Cursor::point(2), RestPolicy::OnGrapheme),
            Cursor::point(2)
        );
    }

    #[test]
    fn on_grapheme_empty_buffer_is_noop() {
        assert_eq!(
            commit("", Cursor::point(0), RestPolicy::OnGrapheme),
            Cursor::point(0)
        );
    }

    #[test]
    fn on_grapheme_pulls_only_head_of_selection() {
        // head at end pulls back to 4; anchor stays put
        assert_eq!(
            commit("hello", Cursor::new(0, 5), RestPolicy::OnGrapheme),
            Cursor::new(0, 4)
        );
    }

    // --- commit: Block -------------------------------------------------------

    #[test]
    fn block_widens_point_to_one_grapheme() {
        assert_eq!(
            commit("hello", Cursor::point(2), RestPolicy::Block),
            Cursor::new(2, 3)
        );
    }

    #[test]
    fn block_widens_over_multibyte_grapheme() {
        // "café": point at start of é (byte 3) widens to cover it, (3,5)
        assert_eq!(
            commit("caf\u{e9}", Cursor::point(3), RestPolicy::Block),
            Cursor::new(3, 5)
        );
    }

    #[test]
    fn block_point_at_end_widens_backward() {
        // no grapheme to the right at the end, so the block covers the last one:
        // point(5) → [4,5), caret on the 'o'
        assert_eq!(
            commit("hello", Cursor::point(5), RestPolicy::Block),
            Cursor::new(4, 5)
        );
    }

    #[test]
    fn block_widens_backward_over_multibyte_at_end() {
        // "café": point at end (5) → block covers the 2-byte é → [3,5)
        assert_eq!(
            commit("caf\u{e9}", Cursor::point(5), RestPolicy::Block),
            Cursor::new(3, 5)
        );
    }

    #[test]
    fn block_empty_buffer_stays_empty() {
        assert_eq!(
            commit("", Cursor::point(0), RestPolicy::Block),
            Cursor::point(0)
        );
    }

    #[test]
    fn block_leaves_existing_selection() {
        assert_eq!(
            commit("hello", Cursor::new(1, 3), RestPolicy::Block),
            Cursor::new(1, 3)
        );
    }

    // --- commit: idempotency across every policy and char boundary -----------

    #[test]
    fn commit_is_idempotent() {
        for policy in [
            RestPolicy::Between,
            RestPolicy::OnGrapheme,
            RestPolicy::Block,
        ] {
            for a in (0..=MIXED.len()).filter(|&i| MIXED.is_char_boundary(i)) {
                for h in (0..=MIXED.len()).filter(|&i| MIXED.is_char_boundary(i)) {
                    let once = commit(MIXED, Cursor::new(a, h), policy);
                    assert_eq!(
                        commit(MIXED, once, policy),
                        once,
                        "policy={policy:?} anchor={a} head={h}"
                    );
                }
            }
        }
    }
}
