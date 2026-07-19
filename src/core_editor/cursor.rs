use crate::core_editor::graphemes::{next_grapheme_boundary, prev_grapheme_boundary};

/// The direction a cursor extends in.
///
/// `Forward` when `head >= anchor`, `Backward` when `head < anchor`.
///
// Part of the selection vocabulary that lands incrementally: no caller until
// selection-aware motions (Vi visual / Helix) are wired through `Cursor`.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Head is at or after anchor.
    Forward,
    /// Head is before anchor.
    Backward,
}

/// Selection intent of a motion — whether it carries the anchor along or drops
/// it. Mirrors Helix's `Movement`; the readable face of `put_cursor`'s old
/// `extend: bool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Movement {
    /// Collapse to a point at the target — an ordinary (non-selecting) motion.
    Move,
    /// Keep the anchor fixed and move only the head — extend the selection.
    Extend,
}

impl Movement {
    /// `Extend` when selecting, `Move` otherwise — the bridge from the boolean
    /// `select` flag the edit commands still carry.
    pub(crate) fn select(select: bool) -> Self {
        if select {
            Movement::Extend
        } else {
            Movement::Move
        }
    }
}

/// Where the caret sits relative to a grapheme — the geometry axis shared by
/// motion resolution ([`resolve_motion`](super::resolve_motion)) and selection
/// extension ([`Cursor::put_cursor`]).
///
/// - `Block` (vi normal / visual): the caret sits *on* a grapheme. A forward
///   word-end lands on the last char, and an inclusive motion's operator or
///   selection covers it.
/// - `Bar` (emacs / vi-insert): the caret sits *between* graphemes, on the
///   trailing boundary, so the same motion is exclusive.
///
/// The readable face of the `block`/`inclusive` boolean that used to thread
/// through the motion path under three different names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaretGeometry {
    Block,
    Bar,
}

impl CaretGeometry {
    /// `true` for `Block` — the caret is on a grapheme, so inclusive motions
    /// cover it. The boolean the geometry-sensitive internals still branch on.
    pub(crate) fn is_inclusive(self) -> bool {
        matches!(self, CaretGeometry::Block)
    }
}

/// How a mode's `Extend` grows a selection — the selection-model axis, chosen
/// per edit mode by [`PromptEditMode::selection_extent`](crate::PromptEditMode).
/// Orthogonal to [`CaretGeometry`]: geometry is *where the caret rests*, extent
/// is *how far a motion drags the head*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionExtent {
    /// Vi visual: the block cursor sweeps the grapheme it lands *on*, so head
    /// covers the motion target: `vw` over `"foo bar"` selects `"foo b"`.
    CoverLanding,
    /// Helix: the selection is the gap-indexed operator span (`op_end`), so the
    /// head stops at the motion boundary: `w` selects `"foo "`, caret on the
    /// space.
    Span,
}

/// Keep the anchor's grapheme covered when a block selection reverses direction
/// across it (Helix `Range::put_cursor`): the anchor hops to the far edge of its
/// grapheme so it stays inside the range. A bar (exclusive) selection is half-open
/// `[min, max)`, so its anchor never moves.
fn flip_anchor(buf: &str, anchor: usize, old_head: usize, new_head: usize, block: bool) -> usize {
    if !block {
        anchor
    } else if old_head >= anchor && new_head < anchor {
        next_grapheme_boundary(buf, anchor)
    } else if old_head < anchor && new_head >= anchor {
        prev_grapheme_boundary(buf, anchor)
    } else {
        anchor
    }
}

/// A cursor as a (possibly empty) range over a buffer.
///
/// Uses gap indexing — `anchor` and `head` represent positions *between* bytes,
/// not bytes themselves. The range covered is left-inclusive and right-
/// exclusive, regardless of anchor/head ordering.
///
/// A "point" cursor (`anchor == head`) is the degenerate empty range and
/// behaves like a plain insertion point. Wider cursors model selections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The anchor of the cursor: the side that doesn't move when extending.
    anchor: usize,
    /// The head of the cursor: moved when extending or moving.
    head: usize,
}

impl Cursor {
    /// Construct a cursor with explicit anchor and head positions.
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// A zero-width cursor at `head`.
    pub fn point(head: usize) -> Self {
        Self::new(head, head)
    }

    /// The anchor position.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The head position. This is what most callers think of as "the cursor".
    pub fn head(&self) -> usize {
        self.head
    }

    /// Start of the range.
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// End of the range.
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    /// `true` when anchor and head are at the same position.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Range algebra that selection-aware motions need but the current point-cursor
/// callers don't yet exercise. Lands incrementally with its callers (Vi visual /
/// Helix selection); kept here, fully tested, so the primitive is complete.
#[allow(dead_code)]
impl Cursor {
    /// `(start, end)` as a `Range<usize>`, for consumers that want it.
    pub fn to_range(self) -> std::ops::Range<usize> {
        self.start()..self.end()
    }

    /// Total byte length of the range.
    pub fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// `Forward` when `head >= anchor`, `Backward` otherwise.
    pub fn direction(&self) -> Direction {
        if self.head < self.anchor {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// Swap anchor and head.
    pub fn flip(self) -> Self {
        Self {
            anchor: self.head,
            head: self.anchor,
        }
    }

    /// Return the cursor if it already points in `direction`, otherwise flip it.
    pub fn with_direction(self, direction: Direction) -> Self {
        if self.direction() == direction {
            self
        } else {
            self.flip()
        }
    }

    /// Move head to `new_head`, preserving anchor. Used for selecting motions.
    pub fn move_head(self, new_head: usize) -> Self {
        Self {
            anchor: self.anchor,
            head: new_head,
        }
    }

    /// Collapse to a point at `head`. Used for non-selecting motions.
    pub fn collapse_to(self, head: usize) -> Self {
        Self::point(head)
    }

    /// Collapse to a point at the current head position.
    pub fn collapse(self) -> Self {
        Self::point(self.head)
    }

    /// Grow the range to cover at least `[from, to]`, preserving anchor/head
    /// ordering.
    ///
    /// If the cursor is currently `Forward`, the anchor can only move left and
    /// the head can only move right. If `Backward`, the roles are inverted.
    pub fn extend(self, from: usize, to: usize) -> Self {
        debug_assert!(from <= to);
        if self.anchor <= self.head {
            Self {
                anchor: self.anchor.min(from),
                head: self.head.max(to),
            }
        } else {
            Self {
                anchor: self.anchor.max(to),
                head: self.head.min(from),
            }
        }
    }

    /// `true` if `pos` lies inside the range (left-inclusive, right-exclusive).
    pub fn contains(&self, pos: usize) -> bool {
        self.start() <= pos && pos < self.end()
    }

    /// The visible caret position
    pub fn caret(&self, buf: &str) -> usize {
        if self.head > self.anchor {
            prev_grapheme_boundary(buf, self.head)
        } else {
            self.head
        }
    }

    /// Move the caret to land on the grapheme at byte `target`, returning the new
    /// cursor. Mirrors Helix's `Range::put_cursor`.
    ///
    /// [`Movement::Move`] collapses to a point at `target` (the commit boundary's
    /// `Block` policy then widens it to a 1-grapheme block). [`Movement::Extend`]
    /// keeps the anchor side — but **flips the anchor onto the other edge of its
    /// grapheme when the selection changes direction**, so the grapheme the
    /// selection started on stays covered (see the type docs / convergence note).
    ///
    /// `geometry` is the per-mode selection geometry: when extending *forward*, a
    /// `Block` (vi-normal) cursor places the head one grapheme past `target` so
    /// `target`'s grapheme is covered; a `Bar` (emacs / Between) cursor places the
    /// head exactly on `target`. It only affects the forward head — a backward
    /// range already covers `target` at its low end, and a collapse ignores it.
    pub fn put_cursor(
        self,
        buf: &str,
        target: usize,
        movement: Movement,
        geometry: CaretGeometry,
    ) -> Self {
        if movement == Movement::Move {
            return Self::point(target);
        }
        let inclusive = geometry.is_inclusive();

        // Flip the anchor onto the far edge of its grapheme when direction
        // changes, so the grapheme the selection started on stays covered.
        let anchor = flip_anchor(buf, self.anchor, self.head, target, inclusive);

        // Place the head so `caret()` lands back on `target`'s grapheme: forward
        // *and inclusive* → head on the far edge; otherwise → head *is* `target`.
        // This forward widening is the `CoverLanding` selection model.
        let head = if anchor <= target && inclusive {
            next_grapheme_boundary(buf, target)
        } else {
            target
        };

        Self::new(anchor, head)
    }

    /// Grow a selection under the [`SelectionExtent::Span`] model: place the head
    /// at the motion's gap-indexed end (`op_end`, with per-motion inclusivity
    /// already baked in by [`resolve_motion`](super::resolve_motion)), with *no*
    /// vi-visual widening, while keeping the anchor grapheme covered through a
    /// block reversal via the same [`flip_anchor`] as [`Cursor::put_cursor`].
    pub(crate) fn extend_span(self, buf: &str, op_end: usize, geometry: CaretGeometry) -> Self {
        let anchor = flip_anchor(buf, self.anchor, self.head, op_end, geometry.is_inclusive());
        Self::new(anchor, op_end)
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::point(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains() {
        let range = Cursor::new(10, 12);

        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(11));
        assert!(!range.contains(12));
        assert!(!range.contains(13));

        let range = Cursor::new(9, 6);
        assert!(!range.contains(9));
        assert!(range.contains(7));
        assert!(range.contains(6));
    }

    #[test]
    fn point_constructs_empty_range_at_head() {
        let range = Cursor::point(5);
        assert_eq!(range.start(), 5);
        assert_eq!(range.end(), 5);
        assert!(range.is_empty());
    }

    #[test]
    fn new_preserves_anchor_and_head_order() {
        let forward = Cursor::new(2, 5);
        assert_eq!(forward.direction(), Direction::Forward);

        let backward = Cursor::new(5, 2);
        assert_eq!(backward.direction(), Direction::Backward);
    }

    #[test]
    fn start_returns_lower_of_anchor_and_head() {
        assert_eq!(Cursor::new(2, 5).start(), 2);
        assert_eq!(Cursor::new(5, 2).start(), 2);
    }

    #[test]
    fn end_returns_higher_of_anchor_and_head() {
        assert_eq!(Cursor::new(2, 5).end(), 5);
        assert_eq!(Cursor::new(5, 2).end(), 5);
    }

    #[test]
    fn start_and_end_agree_for_empty_range() {
        let range = Cursor::point(7);
        assert_eq!(range.start(), range.end());
    }

    #[test]
    fn len_is_zero_for_empty_range() {
        assert_eq!(Cursor::point(7).len(), 0);
    }

    #[test]
    fn len_ignores_direction() {
        assert_eq!(Cursor::new(2, 5).len(), 3);
        assert_eq!(Cursor::new(5, 2).len(), 3);
    }

    #[test]
    fn is_empty_true_when_anchor_equals_head() {
        assert!(Cursor::new(5, 5).is_empty());
        assert!(Cursor::point(0).is_empty());
    }

    #[test]
    fn is_empty_false_for_nonzero_width() {
        assert!(!Cursor::new(2, 5).is_empty());
        assert!(!Cursor::new(5, 2).is_empty());
    }

    #[test]
    fn direction_forward_when_head_greater_than_anchor() {
        assert_eq!(Cursor::new(2, 5).direction(), Direction::Forward);
    }

    #[test]
    fn direction_backward_when_head_less_than_anchor() {
        assert_eq!(Cursor::new(5, 2).direction(), Direction::Backward);
    }

    #[test]
    fn direction_forward_for_empty_range() {
        assert_eq!(Cursor::point(5).direction(), Direction::Forward);
    }

    #[test]
    fn flip_swaps_anchor_and_head() {
        let flipped = Cursor::new(2, 5).flip();
        assert_eq!(flipped, Cursor::new(5, 2));
    }

    #[test]
    fn flip_twice_returns_original() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.flip().flip(), range);
    }

    #[test]
    fn flip_of_empty_range_is_unchanged() {
        let range = Cursor::point(5);
        assert_eq!(range.flip(), range);
    }

    #[test]
    fn with_direction_noop_when_already_forward() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.with_direction(Direction::Forward), range);
    }

    #[test]
    fn with_direction_noop_when_already_backward() {
        let range = Cursor::new(5, 2);
        assert_eq!(range.with_direction(Direction::Backward), range);
    }

    #[test]
    fn with_direction_flips_forward_to_backward() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.with_direction(Direction::Backward), Cursor::new(5, 2));
    }

    #[test]
    fn with_direction_flips_backward_to_forward() {
        let range = Cursor::new(5, 2);
        assert_eq!(range.with_direction(Direction::Forward), Cursor::new(2, 5));
    }

    #[test]
    fn with_direction_on_empty_range_stays_forward() {
        let range = Cursor::point(5);
        assert_eq!(range.with_direction(Direction::Forward), range);
        // Empty range is already Forward; asking for Backward flips it to
        // the same point (anchor == head).
        assert_eq!(range.with_direction(Direction::Backward), range);
    }

    #[test]
    fn extend_forward_shrinks_anchor_left() {
        let range = Cursor::new(5, 8);
        assert_eq!(range.extend(2, 3), Cursor::new(2, 8));
    }

    #[test]
    fn extend_forward_grows_head_right() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.extend(6, 8), Cursor::new(2, 8));
    }

    #[test]
    fn extend_forward_grows_both_sides() {
        let range = Cursor::new(4, 6);
        assert_eq!(range.extend(2, 8), Cursor::new(2, 8));
    }

    #[test]
    fn extend_forward_noop_when_range_already_covers() {
        let range = Cursor::new(1, 9);
        assert_eq!(range.extend(3, 5), range);
    }

    #[test]
    fn extend_backward_preserves_direction() {
        let range = Cursor::new(8, 2);
        let result = range.extend(4, 6);
        assert_eq!(result.direction(), Direction::Backward);
    }

    #[test]
    fn extend_backward_grows_head_left() {
        let range = Cursor::new(8, 5);
        assert_eq!(range.extend(2, 3), Cursor::new(8, 2));
    }

    #[test]
    fn extend_backward_grows_anchor_right() {
        let range = Cursor::new(5, 2);
        assert_eq!(range.extend(6, 8), Cursor::new(8, 2));
    }

    #[test]
    fn extend_from_empty_range_stays_forward() {
        let range = Cursor::point(5);
        let result = range.extend(3, 7);
        assert_eq!(result.direction(), Direction::Forward);
        assert_eq!(result, Cursor::new(3, 7));
    }

    #[test]
    fn extend_with_zero_width_target_is_safe() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.extend(3, 3), range);
    }

    #[test]
    fn contains_false_for_empty_range() {
        let range = Cursor::point(5);
        assert!(!range.contains(5));
        assert!(!range.contains(4));
        assert!(!range.contains(6));
    }

    #[test]
    fn contains_is_direction_agnostic() {
        let forward = Cursor::new(2, 5);
        let backward = Cursor::new(5, 2);
        for pos in 0..=6 {
            assert_eq!(
                forward.contains(pos),
                backward.contains(pos),
                "mismatch at {pos}"
            );
        }
    }

    // --- new accessors and movement helpers ---

    #[test]
    fn anchor_and_head_accessors() {
        let range = Cursor::new(3, 7);
        assert_eq!(range.anchor(), 3);
        assert_eq!(range.head(), 7);
    }

    #[test]
    fn to_range_returns_start_to_end() {
        assert_eq!(Cursor::new(2, 5).to_range(), 2..5);
        assert_eq!(Cursor::new(5, 2).to_range(), 2..5);
        assert_eq!(Cursor::point(4).to_range(), 4..4);
    }

    #[test]
    fn move_head_preserves_anchor() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.move_head(8), Cursor::new(2, 8));
        assert_eq!(range.move_head(0), Cursor::new(2, 0));
    }

    #[test]
    fn move_head_from_point_creates_range() {
        assert_eq!(Cursor::point(3).move_head(7), Cursor::new(3, 7));
    }

    #[test]
    fn collapse_to_drops_anchor_and_moves_head() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.collapse_to(8), Cursor::point(8));
    }

    #[test]
    fn collapse_drops_anchor_at_current_head() {
        let range = Cursor::new(2, 5);
        assert_eq!(range.collapse(), Cursor::point(5));
    }

    #[test]
    fn collapse_on_point_is_noop() {
        let range = Cursor::point(5);
        assert_eq!(range.collapse(), range);
    }

    #[test]
    fn default_is_point_at_zero() {
        assert_eq!(Cursor::default(), Cursor::point(0));
    }

    // --- caret (block-cursor display position) -------------------------------

    #[test]
    fn caret_of_point_is_head() {
        assert_eq!(Cursor::point(2).caret("hello"), 2);
        assert_eq!(Cursor::point(0).caret("hello"), 0);
    }

    #[test]
    fn caret_of_forward_block_retreats_one_grapheme() {
        assert_eq!(Cursor::new(0, 3).caret("hello"), 2);
    }

    #[test]
    fn caret_of_backward_range_is_head() {
        assert_eq!(Cursor::new(5, 2).caret("hello"), 2);
    }

    #[test]
    fn caret_retreats_past_multibyte_grapheme() {
        assert_eq!(Cursor::new(0, 5).caret("café"), 3);
    }

    #[test]
    fn caret_retreats_past_zwj_emoji_as_one() {
        let buf = "a👨‍👩‍👧";
        assert_eq!(Cursor::new(0, buf.len()).caret(buf), 1);
    }

    #[test]
    fn caret_of_empty_buffer_point_is_zero() {
        assert_eq!(Cursor::point(0).caret(""), 0);
    }

    // --- put_cursor (block-cursor caret placement) ---------------------------

    #[test]
    fn put_cursor_non_extend_collapses_to_point() {
        assert_eq!(
            Cursor::new(2, 4).put_cursor("hello", 3, Movement::Move, CaretGeometry::Block),
            Cursor::point(3)
        );
    }

    #[test]
    fn put_cursor_extend_forward_places_head_on_far_edge() {
        // [0,1) extend to byte 2 → [0,3); caret retreats to 2 (the 'l')
        let c = Cursor::new(0, 1).put_cursor("hello", 2, Movement::Extend, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(0, 3));
        assert_eq!(c.caret("hello"), 2);
    }

    #[test]
    fn put_cursor_extend_no_flip_stays_backward() {
        // backward [4,2) extend further left to 1, no crossing → [4,1)
        let c = Cursor::new(4, 2).put_cursor("hello", 1, Movement::Extend, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(4, 1));
        assert_eq!(c.caret("hello"), 1);
    }

    #[test]
    fn put_cursor_extend_flip_forward_to_backward_keeps_anchor_grapheme() {
        // the worked example: [2,4) drag caret to 0 → anchor hops 2→3 → [3,0),
        // so the 'l' at byte 2 (the start grapheme) stays covered.
        let c = Cursor::new(2, 4).put_cursor("hello", 0, Movement::Extend, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(3, 0));
        assert_eq!(c.caret("hello"), 0);
        assert!(c.contains(2)); // start grapheme survived the turn
    }

    #[test]
    fn put_cursor_extend_flip_backward_to_forward_keeps_anchor_grapheme() {
        // backward [4,2) drag caret right to 5 → anchor hops 4→3 → [3,5)
        let c = Cursor::new(4, 2).put_cursor("hello", 5, Movement::Extend, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(3, 5));
        assert_eq!(c.caret("hello"), 4);
        assert!(c.contains(3)); // the 'l' at 3 (start grapheme) survived
    }

    #[test]
    fn put_cursor_extend_flip_across_multibyte_anchor() {
        // "café": é is [3,5). forward [3,5) (on é) drag caret to 0 → anchor hops
        // 3→5 (far edge of é) → [5,0); the whole é stays covered.
        let c = Cursor::new(3, 5).put_cursor("café", 0, Movement::Extend, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(5, 0));
        assert!(c.contains(3) && c.contains(4)); // both bytes of é covered
    }

    // --- extend_span (Span selection model) ----------------------------------

    #[test]
    fn extend_span_bar_grows_head_no_flip() {
        // The emacs / default path: Bar geometry makes flip_anchor a no-op, so
        // the head jumps to op_end and the anchor never moves: [0,3) → [0,4).
        let c = Cursor::new(0, 3).extend_span("hello", 4, CaretGeometry::Bar);
        assert_eq!(c, Cursor::new(0, 4));
    }

    #[test]
    fn extend_span_does_not_widen_like_put_cursor() {
        // The property that separates the two models. From the same [0,1) a
        // forward Extend to 2 under Block: put_cursor (CoverLanding) widens the
        // head onto the landing grapheme's far edge (3); extend_span (Span) stops
        // the head exactly at op_end (2) with no widening.
        let landing =
            Cursor::new(0, 1).put_cursor("hello", 2, Movement::Extend, CaretGeometry::Block);
        assert_eq!(landing, Cursor::new(0, 3));
        let span = Cursor::new(0, 1).extend_span("hello", 2, CaretGeometry::Block);
        assert_eq!(span, Cursor::new(0, 2));
    }

    #[test]
    fn extend_span_block_flip_forward_to_backward_keeps_anchor_grapheme() {
        // Helix groundwork (no current mode pairs Span with Block): a Span that
        // reverses past the anchor still hops it onto the far edge — the same
        // flip_anchor as put_cursor — but the head lands exactly on op_end with
        // no widening. [2,4) → op_end 0 → [3,0).
        let c = Cursor::new(2, 4).extend_span("hello", 0, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(3, 0));
        assert!(c.contains(2)); // start grapheme survives the reversal
    }

    #[test]
    fn extend_span_block_flip_backward_to_forward_keeps_anchor_grapheme() {
        // backward [4,2) Span reverses right to op_end 5 → anchor hops 4→3 → [3,5).
        let c = Cursor::new(4, 2).extend_span("hello", 5, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(3, 5));
        assert!(c.contains(3)); // the 'l' at 3 (start grapheme) survived
    }

    #[test]
    fn extend_span_block_flip_hops_full_multibyte_grapheme() {
        // `extend_span` is a *new* caller into `flip_anchor`; every other Span
        // flip test uses single-byte ASCII where a boundary hop is trivially ±1.
        // Here the anchor sits on a 2-byte grapheme: "café" has é at [3,5). A
        // forward Span [3,5) (on é) that reverses past the anchor (op_end 0) must
        // hop the anchor across the *whole* é (3→5), not one byte, so both of é's
        // bytes stay covered — the same multibyte flip `put_cursor` makes through
        // the shared helper, now proven from the Span entry point too.
        let c = Cursor::new(3, 5).extend_span("café", 0, CaretGeometry::Block);
        assert_eq!(c, Cursor::new(5, 0));
        assert!(c.contains(3) && c.contains(4)); // both bytes of é covered
    }
}
