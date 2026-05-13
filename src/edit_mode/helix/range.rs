#![allow(dead_code)]

/// The direction a range extends in.
///
/// `Forward` when `head >= anchor`, `Backward` when `head < anchor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Forward,
    Backward,
}

/// A selection range.
///
/// Uses gap indexing — `anchor` and `head` represent positions *between* bytes,
/// not bytes themselves. Ranges are inclusive on the left and exclusive on the
/// right, regardless of anchor/head ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HelixRange {
    /// The anchor of the range: the side that doesn't move when extending.
    anchor: usize,
    /// The head of the range, moved when extending.
    head: usize,
}

impl HelixRange {
    pub(super) fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// A zero-width range at `head`.
    pub(super) fn point(head: usize) -> Self {
        Self::new(head, head)
    }

    /// Start of the range
    pub(super) fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// End of the range
    pub(super) fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    /// Total length of the range.
    pub(super) fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// `true` when anchor and head are at the same position.
    pub(super) fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// `Forward` when `head >= anchor`, `Backward` otherwise.
    pub(super) fn direction(&self) -> Direction {
        if self.head < self.anchor {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// Swap anchor and head.
    pub(super) fn flip(self) -> Self {
        Self {
            anchor: self.head,
            head: self.anchor,
        }
    }

    /// Return the range if it already points in `direction`, otherwise flip it.
    pub(super) fn with_direction(self, direction: Direction) -> Self {
        if self.direction() == direction {
            self
        } else {
            self.flip()
        }
    }

    /// Grow the range to cover at least `[from, to]`, preserving anchor/head
    /// ordering.
    ///
    /// If the range is currently `Forward`, the anchor can only move left and
    /// the head can only move right. If `Backward`, the roles are inverted.
    pub(super) fn extend(self, from: usize, to: usize) -> Self {
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
    pub(super) fn contains(&self, pos: usize) -> bool {
        self.start() <= pos && pos < self.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains() {
        let range = HelixRange::new(10, 12);

        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(11));
        assert!(!range.contains(12));
        assert!(!range.contains(13));

        let range = HelixRange::new(9, 6);
        assert!(!range.contains(9));
        assert!(range.contains(7));
        assert!(range.contains(6));
    }

    #[test]
    fn point_constructs_empty_range_at_head() {
        let range = HelixRange::point(5);
        assert_eq!(range.start(), 5);
        assert_eq!(range.end(), 5);
        assert!(range.is_empty());
    }

    #[test]
    fn new_preserves_anchor_and_head_order() {
        let forward = HelixRange::new(2, 5);
        assert_eq!(forward.direction(), Direction::Forward);

        let backward = HelixRange::new(5, 2);
        assert_eq!(backward.direction(), Direction::Backward);
    }

    #[test]
    fn start_returns_lower_of_anchor_and_head() {
        assert_eq!(HelixRange::new(2, 5).start(), 2);
        assert_eq!(HelixRange::new(5, 2).start(), 2);
    }

    #[test]
    fn end_returns_higher_of_anchor_and_head() {
        assert_eq!(HelixRange::new(2, 5).end(), 5);
        assert_eq!(HelixRange::new(5, 2).end(), 5);
    }

    #[test]
    fn start_and_end_agree_for_empty_range() {
        let range = HelixRange::point(7);
        assert_eq!(range.start(), range.end());
    }

    #[test]
    fn len_is_zero_for_empty_range() {
        assert_eq!(HelixRange::point(7).len(), 0);
    }

    #[test]
    fn len_ignores_direction() {
        assert_eq!(HelixRange::new(2, 5).len(), 3);
        assert_eq!(HelixRange::new(5, 2).len(), 3);
    }

    #[test]
    fn is_empty_true_when_anchor_equals_head() {
        assert!(HelixRange::new(5, 5).is_empty());
        assert!(HelixRange::point(0).is_empty());
    }

    #[test]
    fn is_empty_false_for_nonzero_width() {
        assert!(!HelixRange::new(2, 5).is_empty());
        assert!(!HelixRange::new(5, 2).is_empty());
    }

    #[test]
    fn direction_forward_when_head_greater_than_anchor() {
        assert_eq!(HelixRange::new(2, 5).direction(), Direction::Forward);
    }

    #[test]
    fn direction_backward_when_head_less_than_anchor() {
        assert_eq!(HelixRange::new(5, 2).direction(), Direction::Backward);
    }

    #[test]
    fn direction_forward_for_empty_range() {
        assert_eq!(HelixRange::point(5).direction(), Direction::Forward);
    }

    #[test]
    fn flip_swaps_anchor_and_head() {
        let flipped = HelixRange::new(2, 5).flip();
        assert_eq!(flipped, HelixRange::new(5, 2));
    }

    #[test]
    fn flip_twice_returns_original() {
        let range = HelixRange::new(2, 5);
        assert_eq!(range.flip().flip(), range);
    }

    #[test]
    fn flip_of_empty_range_is_unchanged() {
        let range = HelixRange::point(5);
        assert_eq!(range.flip(), range);
    }

    #[test]
    fn with_direction_noop_when_already_forward() {
        let range = HelixRange::new(2, 5);
        assert_eq!(range.with_direction(Direction::Forward), range);
    }

    #[test]
    fn with_direction_noop_when_already_backward() {
        let range = HelixRange::new(5, 2);
        assert_eq!(range.with_direction(Direction::Backward), range);
    }

    #[test]
    fn with_direction_flips_forward_to_backward() {
        let range = HelixRange::new(2, 5);
        assert_eq!(
            range.with_direction(Direction::Backward),
            HelixRange::new(5, 2)
        );
    }

    #[test]
    fn with_direction_flips_backward_to_forward() {
        let range = HelixRange::new(5, 2);
        assert_eq!(
            range.with_direction(Direction::Forward),
            HelixRange::new(2, 5)
        );
    }

    #[test]
    fn with_direction_on_empty_range_stays_forward() {
        let range = HelixRange::point(5);
        assert_eq!(range.with_direction(Direction::Forward), range);
        // Empty range is already Forward, so asking for Backward flips it —
        // which is still the same point, since anchor == head.
        assert_eq!(range.with_direction(Direction::Backward), range);
    }

    #[test]
    fn extend_forward_shrinks_anchor_left() {
        let range = HelixRange::new(5, 8);
        assert_eq!(range.extend(2, 3), HelixRange::new(2, 8));
    }

    #[test]
    fn extend_forward_grows_head_right() {
        let range = HelixRange::new(2, 5);
        assert_eq!(range.extend(6, 8), HelixRange::new(2, 8));
    }

    #[test]
    fn extend_forward_grows_both_sides() {
        let range = HelixRange::new(4, 6);
        assert_eq!(range.extend(2, 8), HelixRange::new(2, 8));
    }

    #[test]
    fn extend_forward_noop_when_range_already_covers() {
        let range = HelixRange::new(1, 9);
        assert_eq!(range.extend(3, 5), range);
    }

    #[test]
    fn extend_backward_preserves_direction() {
        let range = HelixRange::new(8, 2);
        let result = range.extend(4, 6);
        assert_eq!(result.direction(), Direction::Backward);
    }

    #[test]
    fn extend_backward_grows_head_left() {
        let range = HelixRange::new(8, 5);
        assert_eq!(range.extend(2, 3), HelixRange::new(8, 2));
    }

    #[test]
    fn extend_backward_grows_anchor_right() {
        let range = HelixRange::new(5, 2);
        assert_eq!(range.extend(6, 8), HelixRange::new(8, 2));
    }

    #[test]
    fn extend_from_empty_range_stays_forward() {
        let range = HelixRange::point(5);
        let result = range.extend(3, 7);
        assert_eq!(result.direction(), Direction::Forward);
        assert_eq!(result, HelixRange::new(3, 7));
    }

    #[test]
    fn extend_with_zero_width_target_is_safe() {
        let range = HelixRange::new(2, 5);
        assert_eq!(range.extend(3, 3), range);
    }

    #[test]
    fn contains_false_for_empty_range() {
        let range = HelixRange::point(5);
        assert!(!range.contains(5));
        assert!(!range.contains(4));
        assert!(!range.contains(6));
    }

    #[test]
    fn contains_is_direction_agnostic() {
        let forward = HelixRange::new(2, 5);
        let backward = HelixRange::new(5, 2);
        for pos in 0..=6 {
            assert_eq!(
                forward.contains(pos),
                backward.contains(pos),
                "mismatch at {pos}"
            );
        }
    }
}
