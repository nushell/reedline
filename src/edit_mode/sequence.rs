use super::keybindings::{KeyCombination, Keybindings};
use crate::ReedlineEvent;

/// What one key did to a pending sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceResolution {
    /// The keys so far are a prefix of a longer sequence (and possibly a
    /// complete binding too). Wait for the next key or the engine's timeout.
    Pending,
    /// The sequence completed; the event should run.
    Complete(ReedlineEvent),
    /// The key does not continue the sequence. `flush` holds the keys that
    /// were pending *before* it; the current key was not consumed and the mode
    /// should hand it to its ordinary single-key path.
    NoMatch {
        /// Keys to replay through the mode's fallback, in order.
        flush: Vec<KeyCombination>,
    },
}

/// Tracks a half-typed multi-key sequence.
///
/// The resolver only knows the keys; the mode supplies its own keybinding
/// table on every call and decides what a flushed prefix means. That keeps one
/// implementation shared by emacs, vi and any custom mode, while letting vi
/// fall back to self-insert in insert mode and to its table in normal mode.
#[derive(Debug, Default, Clone)]
pub struct SequenceResolver {
    pending: Vec<KeyCombination>,
}

impl SequenceResolver {
    /// A resolver with no pending keys.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a sequence is half typed.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The keys collected so far.
    pub fn pending(&self) -> &[KeyCombination] {
        &self.pending
    }

    /// Drop a half-typed sequence without resolving it.
    pub fn reset(&mut self) {
        self.pending.clear();
    }

    /// Take the pending keys, leaving the resolver empty.
    ///
    /// The caller decides how to flush them: a complete sequence inside the
    /// prefix wins, otherwise they are replayed through the mode's fallback.
    pub fn take_pending(&mut self) -> Vec<KeyCombination> {
        std::mem::take(&mut self.pending)
    }

    /// Feed one key to the sequence.
    ///
    /// A path that is both a complete binding and a prefix stays `Pending`, so
    /// `j` can be a binding while `j j` is a longer sequence. On a miss the
    /// current key is left unconsumed: the returned `flush` contains only the
    /// keys that were already pending.
    pub fn resolve(
        &mut self,
        keybindings: &Keybindings,
        key: KeyCombination,
    ) -> SequenceResolution {
        self.pending.push(key);

        let lookup = keybindings.lookup_sequence(&self.pending);
        if lookup.is_prefix {
            return SequenceResolution::Pending;
        }
        if let Some(event) = lookup.exact {
            self.pending.clear();
            return SequenceResolution::Complete(event);
        }

        // The key is not part of the sequence. Hand back the prefix that was
        // pending before it; the caller re-feeds the current key.
        self.pending.pop();
        let flush = std::mem::take(&mut self.pending);
        SequenceResolution::NoMatch { flush }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_mode::keybindings::Keybindings;
    use crossterm::event::{KeyCode, KeyModifiers};
    use pretty_assertions::assert_eq;

    fn combo(c: char) -> KeyCombination {
        KeyCombination {
            modifier: KeyModifiers::NONE,
            key_code: KeyCode::Char(c),
        }
    }

    fn two_key_table() -> Keybindings {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        kb
    }

    #[test]
    fn complete_sequence_returns_the_event() {
        let kb = two_key_table();
        let mut resolver = SequenceResolver::new();

        assert_eq!(
            resolver.resolve(&kb, combo('j')),
            SequenceResolution::Pending
        );
        assert_eq!(
            resolver.resolve(&kb, combo('j')),
            SequenceResolution::Complete(ReedlineEvent::Esc)
        );
        assert!(!resolver.is_pending());
    }

    #[test]
    fn a_prefix_that_is_also_a_binding_stays_pending() {
        let mut kb = Keybindings::new();
        kb.add_binding(KeyModifiers::NONE, KeyCode::Char('j'), ReedlineEvent::Down);
        kb.add_binding_sequence(&[combo('j'), combo('j')], ReedlineEvent::Esc);
        let mut resolver = SequenceResolver::new();

        // `j` is bound, but it is also the start of `jj`, so it waits.
        assert_eq!(
            resolver.resolve(&kb, combo('j')),
            SequenceResolution::Pending
        );
        assert_eq!(
            resolver.resolve(&kb, combo('j')),
            SequenceResolution::Complete(ReedlineEvent::Esc)
        );
    }

    #[test]
    fn a_miss_flushes_the_prefix_but_not_the_current_key() {
        let kb = two_key_table();
        let mut resolver = SequenceResolver::new();

        assert_eq!(
            resolver.resolve(&kb, combo('j')),
            SequenceResolution::Pending
        );
        assert_eq!(
            resolver.resolve(&kb, combo('x')),
            SequenceResolution::NoMatch {
                flush: vec![combo('j')]
            }
        );
        // `x` was not consumed: the resolver is empty for the caller to retry.
        assert!(!resolver.is_pending());
    }

    #[test]
    fn an_unrelated_key_with_nothing_pending_flushes_nothing() {
        let kb = two_key_table();
        let mut resolver = SequenceResolver::new();

        assert_eq!(
            resolver.resolve(&kb, combo('x')),
            SequenceResolution::NoMatch { flush: vec![] }
        );
    }

    #[test]
    fn a_longer_sequence_wins_over_a_shorter_complete_one() {
        let mut kb = Keybindings::new();
        kb.add_binding_sequence(&[combo('a'), combo('b')], ReedlineEvent::Up);
        kb.add_binding_sequence(&[combo('a'), combo('b'), combo('c')], ReedlineEvent::Down);
        let mut resolver = SequenceResolver::new();

        assert_eq!(
            resolver.resolve(&kb, combo('a')),
            SequenceResolution::Pending
        );
        // `ab` is complete, but `abc` continues, so it waits.
        assert_eq!(
            resolver.resolve(&kb, combo('b')),
            SequenceResolution::Pending
        );
        assert_eq!(
            resolver.resolve(&kb, combo('c')),
            SequenceResolution::Complete(ReedlineEvent::Down)
        );
    }

    #[test]
    fn take_pending_leaves_the_resolver_empty() {
        let kb = two_key_table();
        let mut resolver = SequenceResolver::new();
        let _ = resolver.resolve(&kb, combo('j'));

        assert_eq!(resolver.take_pending(), vec![combo('j')]);
        assert!(!resolver.is_pending());
        assert!(resolver.pending().is_empty());
    }

    #[test]
    fn reset_drops_a_pending_sequence() {
        let kb = two_key_table();
        let mut resolver = SequenceResolver::new();
        let _ = resolver.resolve(&kb, combo('j'));

        resolver.reset();
        assert!(!resolver.is_pending());
    }
}
