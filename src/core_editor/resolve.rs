use crate::{
    core_editor::{
        graphemes::{next_grapheme_boundary, prev_grapheme_boundary},
        line, word, Cursor,
    },
    enums::{Direction, MotionTarget, WordEdge},
    FindStop,
};

/// A resolved motion, as two byte positions:
/// - `head` — where the cursor lands (used by `Move`/`Extend`).
/// - `op_end` — the far edge an operator consumes (used by `Cut`/`Copy`/`Erase`).
///
/// They differ only for *inclusive* motions: a forward word-end (`e`) or find
/// (`f`/`t`) lands the cursor *on* a grapheme, but an operator eats it — so
/// `op_end` is one grapheme past `head`. For exclusive motions `op_end == head`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Movement {
    pub(crate) head: usize,
    pub(crate) op_end: usize,
}

/// The span an operator (`Cut`/`Copy`/`Erase`) acts over: a [`Cursor`] from
/// `origin` to the motion's `op_end`. `start()..end()` is the byte range to
/// consume — inclusivity and direction are already baked into `op_end`, so the
/// operator never has to reconsider them.
pub(crate) fn operator_span(buf: &str, origin: usize, target: MotionTarget) -> Cursor {
    Cursor::new(origin, resolve_motion(buf, origin, target).op_end)
}

/// Resolve a public [`MotionTarget`] against `buf`, relative to `origin`.
///
/// Total over every variant — a target with no resolution yet (`Find`) stays at
/// `origin` (a no-op) rather than panicking, so a target constructed from config
/// or another mode can never crash the editor. Context-aware (takes `buf`), so
/// line/buffer edges resolve correctly where a context-free conversion couldn't.
pub(crate) fn resolve_motion(buf: &str, origin: usize, target: MotionTarget) -> Movement {
    let span = |head: usize, inclusive: bool| Movement {
        head,
        op_end: if inclusive {
            next_grapheme_boundary(buf, head)
        } else {
            head
        },
    };
    match target {
        MotionTarget::Grapheme(Direction::Forward) => {
            span(next_grapheme_boundary(buf, origin), false)
        }
        MotionTarget::Grapheme(Direction::Backward) => {
            span(prev_grapheme_boundary(buf, origin), false)
        }
        MotionTarget::Word {
            kind,
            edge,
            direction,
        } => {
            let head = word::locate_word(buf, origin, kind, edge, direction == Direction::Forward);
            let inclusive = edge == WordEdge::End && direction == Direction::Forward;
            span(head, inclusive)
        }
        MotionTarget::Offset(n) => span(n.min(buf.len()), false),
        MotionTarget::BufferEdge(Direction::Backward) => span(0, false),
        MotionTarget::BufferEdge(Direction::Forward) => span(buf.len(), false),
        MotionTarget::LineEdge(Direction::Backward) => {
            span(line::start_of_line(buf, origin), false)
        }
        // CRLF-aware via `end_of_line`: `$` stops before the `\r` of a `\r\n`
        // terminator, matching `LineBuffer::find_current_line_end`.
        MotionTarget::LineEdge(Direction::Forward) => span(line::end_of_line(buf, origin), false),
        // The adjacent line (`j`/`k`). Lands on the *start* of the line below /
        // above; on the first/last line it stays put (so `dj`/`dk` there only
        // affect the current line). Operators snap the span to whole lines.
        MotionTarget::Line(Direction::Forward) => {
            let head = line::start_of_next_line(buf, origin).unwrap_or(origin);
            span(head, false)
        }
        MotionTarget::Line(Direction::Backward) => {
            let line_start = line::start_of_line(buf, origin);
            let head = if line_start == 0 {
                origin
            } else {
                line::start_of_line(buf, line_start - 1)
            };
            span(head, false)
        }
        // Character search (vi `f`/`t`/`F`/`T`). A miss stays at `origin` (a
        // no-op) rather than panicking. Forward find is inclusive (`df` eats the
        // target char); backward is exclusive.
        MotionTarget::Find {
            ch,
            direction,
            stop,
        } => {
            let hit = find_char(buf, origin, ch, direction, stop);
            let inclusive = hit.is_some() && direction == Direction::Forward;
            span(hit.unwrap_or(origin), inclusive)
        }
    }
}

// we either find it or not.
fn find_char(
    buf: &str,
    origin: usize,
    ch: char,
    direction: Direction,
    stop: FindStop,
) -> Option<usize> {
    let hit = match direction {
        Direction::Forward => {
            let start = next_grapheme_boundary(buf, origin);
            buf[start..].find(ch).map(|rel| start + rel)
        }
        Direction::Backward => buf[..origin].rfind(ch),
    }?;

    Some(match (direction, stop) {
        (_, FindStop::On) => hit,
        (Direction::Forward, FindStop::Before) => prev_grapheme_boundary(buf, hit),
        (Direction::Backward, FindStop::Before) => next_grapheme_boundary(buf, hit),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WordKind;

    fn word(edge: WordEdge, direction: Direction) -> MotionTarget {
        MotionTarget::Word {
            kind: WordKind::Small,
            edge,
            direction,
        }
    }

    #[test]
    fn resolve_motion_marks_forward_word_end_inclusive() {
        // Only a forward word *end* is inclusive; starts and backward motions are not.
        // forward word-end is inclusive: lands on the last 'o' (2), op_end one past (3)
        let m = resolve_motion("foo bar", 0, word(WordEdge::End, Direction::Forward));
        assert_eq!(m, Movement { head: 2, op_end: 3 });
        // starts and backward motions are exclusive: op_end == head
        let m = resolve_motion("foo bar", 0, word(WordEdge::Start, Direction::Forward));
        assert_eq!(m.op_end, m.head);
        let m = resolve_motion("foo bar", 7, word(WordEdge::End, Direction::Backward));
        assert_eq!(m.op_end, m.head);
    }

    #[test]
    fn resolve_motion_handles_line_and_buffer_edges() {
        let buf = "ab\ncd\nef";
        // line edges resolve against the *current* line (context-aware)
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::LineEdge(Direction::Backward)).head,
            3
        );
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::LineEdge(Direction::Forward)).head,
            5
        );
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::BufferEdge(Direction::Backward)).head,
            0
        );
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::BufferEdge(Direction::Forward)).head,
            8
        );
    }

    use crate::enums::FindStop;

    /// Build a `Find` target — the `f`/`t`/`F`/`T` family.
    fn find(ch: char, direction: Direction, stop: FindStop) -> MotionTarget {
        MotionTarget::Find {
            ch,
            direction,
            stop,
        }
    }

    #[test]
    fn resolve_motion_find_forward_on_lands_on_char() {
        // `foo bar`:  f0 o1 o2 _3 b4 a5 r6
        // `f b` — land *on* the next `b` after origin.
        // Forward find is an inclusive motion (vim `f`/`t`).
        assert_eq!(
            resolve_motion("foo bar", 0, find('b', Direction::Forward, FindStop::On)),
            Movement { head: 4, op_end: 5 } // inclusive: op_end one past 'b'
        );
    }

    #[test]
    fn resolve_motion_find_forward_before_stops_short() {
        // `t b` — stop one grapheme *short* of the next `b` (byte 3).
        assert_eq!(
            resolve_motion(
                "foo bar",
                0,
                find('b', Direction::Forward, FindStop::Before)
            ),
            Movement { head: 3, op_end: 4 } // inclusive: op_end one past byte 3
        );
    }

    #[test]
    fn resolve_motion_find_backward_on_lands_on_char() {
        // `F f` from `r` (origin 6) — land *on* the previous `f` (byte 0).
        // Backward find is an exclusive motion (vim `F`/`T`).
        assert_eq!(
            resolve_motion("foo bar", 6, find('f', Direction::Backward, FindStop::On)),
            Movement { head: 0, op_end: 0 } // backward is exclusive
        );
    }

    #[test]
    fn resolve_motion_find_backward_before_stops_short() {
        // `T f` from origin 6 — stop one grapheme short, i.e. just *after*
        // the `f` (byte 1).
        assert_eq!(
            resolve_motion(
                "foo bar",
                6,
                find('f', Direction::Backward, FindStop::Before)
            ),
            Movement { head: 1, op_end: 1 } // backward is exclusive
        );
    }

    #[test]
    fn resolve_motion_find_searches_strictly_past_origin() {
        // The char *at* origin doesn't count — search starts past it, like
        // `locate_word`. Origin 4 is `b`; forward-find `b` skips it and,
        // finding no other, stays put.
        assert_eq!(
            resolve_motion("foo bar", 4, find('b', Direction::Forward, FindStop::On)).head,
            4
        );
    }

    #[test]
    fn resolve_motion_find_before_replay_from_landing_spot_is_stuck() {
        // `t` lands one grapheme short of the target; replaying the same Find
        // (`;`) from that landing spot searches from the next grapheme — the
        // target char itself — re-finds the *same* occurrence, and lands back
        // where it began. Vim (default cpoptions) skips to the next occurrence
        // instead; reedline keeps the historical stuck behavior, pinned here so
        // any future change to it is deliberate.
        let t = find('x', Direction::Forward, FindStop::Before);
        // "axbxc": x@1, x@3. From 0 (adjacent to x@1): stays at 0.
        assert_eq!(resolve_motion("axbxc", 0, t).head, 0);
        // From 2 (adjacent to x@3): stays at 2.
        assert_eq!(resolve_motion("axbxc", 2, t).head, 2);
    }

    #[test]
    fn resolve_motion_find_absent_char_stays_put() {
        // Totality: an unfindable char is a no-op, never a panic.
        assert_eq!(
            resolve_motion("foo bar", 3, find('z', Direction::Forward, FindStop::On)),
            Movement { head: 3, op_end: 3 } // miss: no-op at origin
        );
    }

    #[test]
    fn resolve_motion_find_before_respects_grapheme_boundaries() {
        // `a→b`:  a0  →1..4 (3-byte arrow)  b4.  `t b` must land at the
        // *start* of `→` (byte 1), not byte 3 — proof the impl steps a
        // grapheme, not a single byte.
        assert_eq!(
            resolve_motion("a→b", 0, find('b', Direction::Forward, FindStop::Before)).head,
            1
        );
        // backward `T a` from `b` (origin 4): one grapheme *after* `a` is
        // also the start of `→` (byte 1).
        assert_eq!(
            resolve_motion("a→b", 4, find('a', Direction::Backward, FindStop::Before)).head,
            1
        );
    }

    #[test]
    fn resolve_motion_find_backward_finds_adjacent_char() {
        // `fab`:  f0 a1 b2.  `F a` from `b` (origin 2) must land on the `a`
        // *immediately* left of the cursor (byte 1) — the backward search
        // looks at the char right before origin, it does not skip a grapheme.
        assert_eq!(
            resolve_motion("fab", 2, find('a', Direction::Backward, FindStop::On)).head,
            1
        );
    }

    #[test]
    fn resolve_motion_find_backward_searches_strictly_before_origin() {
        // Mirror of the forward case: the char *at* origin is excluded. Origin
        // 0 is `b`; backward-find `b` has nothing before it and stays put.
        assert_eq!(
            resolve_motion("bab", 0, find('b', Direction::Backward, FindStop::On)).head,
            0
        );
    }

    // --- line / buffer edges (`0`/`$`/`gg`/`G`) ---
    //
    // The whole reason `LineEdge` and `BufferEdge` are distinct targets is
    // multiline: `$` must stop at the next `\n`, not run to the buffer end.
    // `"ab\ncd"` has bytes a0 b1 \n2 c3 d4, len 5.

    #[test]
    fn resolve_motion_line_edge_forward_stops_at_newline() {
        // `$` from inside the first line lands *at* the `\n`, not the buffer end.
        assert_eq!(
            resolve_motion("ab\ncd", 0, MotionTarget::LineEdge(Direction::Forward)),
            Movement { head: 2, op_end: 2 } // line edge is exclusive
        );
    }

    #[test]
    fn resolve_motion_line_edge_forward_stops_before_crlf() {
        // On a CRLF-terminated line `$` lands before the `\r`, matching
        // `LineBuffer::find_current_line_end` — both delegate to `end_of_line`.
        assert_eq!(
            resolve_motion("ab\r\ncd", 0, MotionTarget::LineEdge(Direction::Forward)).head,
            2
        );
    }

    #[test]
    fn resolve_motion_line_edge_backward_stops_at_line_start() {
        // `0` from the second line lands at that line's start (byte 3), not 0.
        assert_eq!(
            resolve_motion("ab\ncd", 4, MotionTarget::LineEdge(Direction::Backward)).head,
            3
        );
    }

    #[test]
    fn resolve_motion_buffer_edge_spans_whole_buffer() {
        // `G` / `gg` ignore line breaks — start is 0, end is the buffer length.
        assert_eq!(
            resolve_motion("ab\ncd", 0, MotionTarget::BufferEdge(Direction::Forward)).head,
            5
        );
        assert_eq!(
            resolve_motion("ab\ncd", 4, MotionTarget::BufferEdge(Direction::Backward)).head,
            0
        );
    }

    #[test]
    fn resolve_motion_line_targets_the_adjacent_line() {
        let buf = "ab\ncd\nef"; // ab@0-1 \n@2 cd@3-4 \n@5 ef@6-7
                                // from "cd" (origin 4): down → start of "ef", up → start of "ab"
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::Line(Direction::Forward)).head,
            6
        );
        assert_eq!(
            resolve_motion(buf, 4, MotionTarget::Line(Direction::Backward)).head,
            0
        );
        // no adjacent line → stay put (last line down, first line up)
        assert_eq!(
            resolve_motion(buf, 7, MotionTarget::Line(Direction::Forward)).head,
            7
        );
        assert_eq!(
            resolve_motion(buf, 1, MotionTarget::Line(Direction::Backward)).head,
            1
        );
    }
}
