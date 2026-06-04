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
}
