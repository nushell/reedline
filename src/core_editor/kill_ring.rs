//! Bounded kill ring backing [`EditCommand::YankPop`](crate::EditCommand::YankPop).
//!
//! Every kill/cut *and* copy operation records its text at the front of the
//! ring (GNU readline's `kill-ring-save`, `M-w`, also copies into the kill
//! ring), while the existing cut buffer still holds only the newest entry.
//! `PasteCutBufferBefore`/`PasteCutBufferAfter` therefore keep their old
//! semantics: they always paste the front of the ring.
//!
//! The ring keeps at most [`KILL_RING_CAPACITY`] entries; recording a new kill
//! when full drops the oldest one.
//!
//! # `yank-pop` precondition
//!
//! `YankPop` only rotates when it is the command immediately following a
//! yank (`PasteCutBufferBefore`, its alias `Paste`, or
//! `PasteCutBufferAfter`). The editor tracks the byte span of that yank and
//! the ring index it came from; any other command clears that state, so a
//! later `YankPop` is a no-op. Repeated `YankPop`s walk from newest to oldest
//! and wrap around to the newest entry again. With an empty ring `YankPop` is
//! always a no-op.

use std::collections::VecDeque;

use super::Clipboard;
use crate::Granularity;

/// Maximum number of kills retained by the ring.
pub(crate) const KILL_RING_CAPACITY: usize = 16;

/// Newest-first history of killed/copied text.
#[derive(Debug, Default)]
pub(crate) struct KillRing {
    entries: VecDeque<(String, Granularity)>,
}

impl KillRing {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Store `content` in `clipboard` and push it onto the front of the ring.
    ///
    /// Keeping both in sync is what lets a paste start at index 0: the cut
    /// buffer always equals the ring's front entry.
    pub(crate) fn record(
        &mut self,
        clipboard: &mut dyn Clipboard,
        content: &str,
        granularity: Granularity,
    ) {
        clipboard.set(content, granularity);
        if self.entries.len() == KILL_RING_CAPACITY {
            self.entries.pop_back();
        }
        self.entries.push_front((content.to_owned(), granularity));
    }

    /// Whether the newest entry is exactly `content` with `granularity`.
    ///
    /// Used to arm yank rotation only when the pasted text really came from
    /// the ring (a cut buffer written directly by a test or external caller
    /// has no history to rotate through).
    pub(crate) fn top_is(&self, content: &str, granularity: Granularity) -> bool {
        self.entries
            .front()
            .is_some_and(|(entry, entry_granularity)| {
                entry == content && *entry_granularity == granularity
            })
    }

    /// Entry at `index`, where 0 is the newest.
    pub(crate) fn get(&self, index: usize) -> Option<(&str, Granularity)> {
        self.entries
            .get(index)
            .map(|(content, granularity)| (content.as_str(), *granularity))
    }

    /// The next index to rotate to, wrapping back to the newest entry.
    pub(crate) fn next_index(&self, index: usize) -> Option<usize> {
        if self.entries.is_empty() {
            None
        } else {
            Some((index + 1) % self.entries.len())
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_editor::get_local_clipboard;

    #[test]
    fn newest_first_and_bounded() {
        let mut ring = KillRing::new();
        let mut clipboard = get_local_clipboard();

        for i in 0..KILL_RING_CAPACITY + 3 {
            ring.record(clipboard.as_mut(), &i.to_string(), Granularity::CharWise);
        }

        let newest = (KILL_RING_CAPACITY + 2).to_string();
        assert_eq!(ring.len(), KILL_RING_CAPACITY);
        assert_eq!(
            ring.get(0).map(|(content, _)| content),
            Some(newest.as_str())
        );
        // Oldest three entries were evicted.
        assert_eq!(
            ring.get(KILL_RING_CAPACITY - 1).map(|(content, _)| content),
            Some("3")
        );
        // The cut buffer mirrors the front entry.
        assert_eq!(clipboard.get().0, (KILL_RING_CAPACITY + 2).to_string());
    }

    #[test]
    fn rotation_wraps() {
        let mut ring = KillRing::new();
        let mut clipboard = get_local_clipboard();
        ring.record(clipboard.as_mut(), "a", Granularity::CharWise);
        ring.record(clipboard.as_mut(), "b", Granularity::CharWise);

        assert_eq!(ring.next_index(0), Some(1));
        assert_eq!(ring.next_index(1), Some(0));
        assert_eq!(KillRing::new().next_index(0), None);
    }
}
