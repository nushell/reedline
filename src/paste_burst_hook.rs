//! A generic paste-burst timing hook.
//!
//! Some terminals (notably Warp and Windows ConPTY) deliver a paste not as one
//! bracketed-paste [`Event::Paste`](crossterm::event::Event) but as a rapid
//! stream of individual key events. The read loop cannot tell such a stream from
//! fast human typing by content alone — it needs the arrival *timing*. This hook
//! lets a host own that timing oracle: chars keep echoing into the line buffer
//! live (no latency), and the host's detector answers two questions the read
//! loop asks — is a bare `Enter` a paste-embedded newline (insert `\n`) rather
//! than a settling submit, and is a real burst still coalescing (keep draining)?
//!
//! When a `PasteBurstHook` is installed on the [`Reedline`](crate::Reedline)
//! engine via [`with_paste_burst`](crate::Reedline::with_paste_burst), the read
//! loop feeds each just-read plain char to [`PasteBurstHook::on_char`] at read
//! time (so real inter-char timing is preserved), keeps draining while
//! [`PasteBurstHook::is_burst_active`] is true (using
//! [`PasteBurstHook::poll_timeout`] as the idle-flush window), calls
//! [`PasteBurstHook::settle`] once the burst goes idle, and reclassifies a bare
//! `Enter` to an inserted newline when [`PasteBurstHook::enter_is_newline`]
//! returns true.
//!
//! The trait is intentionally generic (no application-specific concepts). The
//! detector state and timing thresholds live entirely on the host side; reedline
//! only drives the hook from the read loop.
//!
//! This is a fallback for terminals that do not emit `Event::Paste`; where
//! bracketed paste is available, prefer [`Reedline::use_bracketed_paste`].

use std::time::Duration;

/// A host hook that classifies a rapid key-event stream as a paste burst using
/// arrival timing. Installed on the [`Reedline`](crate::Reedline) engine via
/// [`with_paste_burst`](crate::Reedline::with_paste_burst); when absent, the
/// read loop behaves exactly as before.
///
/// Must be `Send + Sync` because it is held behind an `Arc` on the `Reedline`
/// engine, which is moved across the read loop. Every method takes `&self`; the
/// implementation is expected to hold its mutable detector state behind interior
/// mutability (e.g. a `Mutex`) and to read the clock itself.
pub trait PasteBurstHook: Send + Sync {
    /// Feed one just-read plain char to the burst detector. Called at read time
    /// so the detector sees real inter-char timing.
    fn on_char(&self, c: char);

    /// Decide AND record whether a bare `Enter` arriving now is a paste-embedded
    /// newline (insert `\n`, do not submit) rather than a settling submit.
    fn enter_is_newline(&self) -> bool;

    /// True while a real paste burst is coalescing — the read loop keeps
    /// draining the event queue instead of processing the batch.
    fn is_burst_active(&self) -> bool;

    /// Poll timeout to use while draining an active burst (the idle-flush
    /// window). When a poll of this duration finds no new event, the burst has
    /// settled.
    fn poll_timeout(&self) -> Duration;

    /// Reset detector state after a batch settles, so the next line starts
    /// clean.
    fn settle(&self);

    /// Resolve a settled paste burst. Given the coalesced burst text (embedded
    /// newlines already `\n`), the host may reference-ify it: return `Some(s)` to
    /// have the read loop insert `s` (a `[Pasted text #N, +M lines]` placeholder —
    /// the host stored the original out-of-band) INSTEAD of the raw burst text,
    /// or `None` to keep the raw text. Called at most once per burst batch.
    fn resolve_burst(&self, coalesced: &str) -> Option<String>;
}
