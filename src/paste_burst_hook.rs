//! A generic paste-burst timing hook.
//!
//! Some terminals (notably Warp and Windows ConPTY) deliver a paste not as one
//! bracketed-paste [`Event::Paste`](crossterm::event::Event) but as a rapid
//! stream of individual key events. The read loop cannot tell such a stream from
//! fast human typing by content alone — it needs the arrival *timing*. This hook
//! lets a host own that timing oracle: chars echo as usual until the detector
//! declares a burst, from then on the rest of the paste lands as one insert
//! once it goes idle. The host's detector answers two questions the read loop
//! asks: is a bare `Enter` a paste-embedded newline (insert `\n`) rather than a
//! settling submit, and is a real burst still coalescing (keep draining)?
//! `Ctrl-J`, which is how a raw LF arrives in raw mode, counts as `Enter`
//! throughout.
//!
//! When a `PasteBurstHook` is installed on the [`Reedline`](crate::Reedline)
//! engine via [`with_paste_burst`](crate::Reedline::with_paste_burst), the read
//! loop feeds each just-read plain char to [`PasteBurstHook::on_char`] at read
//! time (so real inter-char timing is preserved), keeps draining while
//! [`PasteBurstHook::is_burst_active`] is true (using
//! [`PasteBurstHook::poll_timeout`] as the idle-flush window), and calls
//! [`PasteBurstHook::settle`] once after each burst, when the batch holding it
//! has been processed. A batch without a burst is not followed by `settle`, so
//! the detector has to expire stale timing itself. A bare `Enter` drained
//! into a detected burst is always coalesced as an embedded newline; outside a
//! detected burst (a short paste that never reached the burst threshold), a
//! bare `Enter` is instead reclassified to an inserted newline when
//! [`PasteBurstHook::enter_is_newline`] returns true.
//!
//! # Limitations
//!
//! A paste that starts with a newline has that newline arrive before any char,
//! so a detector that works from timing has nothing to compare it against. It
//! is handled as an ordinary `Enter`, which on an empty buffer submits an empty
//! line.
//!
//! On Unix, crossterm's default event source reads 1024 bytes per readiness
//! notification and leaves the rest unread until the next one. Without a hook
//! this goes unnoticed: every pasted newline submits, and the terminal's reply
//! to the next prompt's cursor-position query is new input that wakes the
//! reader again. With a hook nothing submits mid-paste, so a paste larger than
//! that stalls part-way until the next key press, and the hook's poll does not
//! see the unread bytes either. The fix belongs in crossterm, which would have
//! to read until the descriptor runs dry. The event source enabled by
//! crossterm's `use-dev-tty` feature does not stall. Where the terminal
//! supports bracketed paste, prefer it.
//!
//! An event source that hands over one event per batch, like the `use-dev-tty`
//! one, gives the detector its chars one batch at a time. Those that arrive
//! before it declares a burst are inserted as typed, and only the rest of the
//! paste is coalesced and passed to [`PasteBurstHook::resolve_burst`].
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
/// mutability (e.g. a `Mutex`).
///
/// Only [`on_char`](Self::on_char) is called at the moment the input it
/// describes arrives, so it is the one place where reading the clock measures
/// what it looks like it measures. [`is_burst_active`](Self::is_burst_active)
/// is called at a point in the read loop that is separated from the input by
/// an idle poll — see that method for what it must answer from recorded state
/// instead. [`enter_is_newline`](Self::enter_is_newline) is only asked outside
/// a detected burst, where no such separation applies; see that method for
/// details.
pub trait PasteBurstHook: Send + Sync {
    /// Feed one just-read plain char to the burst detector. Called at read time
    /// so the detector sees real inter-char timing.
    fn on_char(&self, c: char);

    /// Decide whether a bare `Enter` in the batch being processed is a
    /// paste-embedded newline (insert `\n`, do not submit) rather than a
    /// settling submit.
    ///
    /// # When this is called
    ///
    /// The engine only asks this outside a detected burst: a short, fast
    /// paste (e.g. `aa\nbb`) whose lines never reach the detector's burst
    /// threshold is never drained past the ordinary end-of-batch stop, so the
    /// batch (and this question) follows the chars immediately, with no idle
    /// poll in between. An inter-char freshness test — "did a char arrive
    /// within the last N ms" — is therefore the right signal here, and
    /// correctly keeps the paste from submitting halfway through.
    ///
    /// Since a batch without a burst is not followed by [`settle`](Self::settle),
    /// the timing it measures carries over from earlier batches. An `Enter`
    /// typed within that window of the previous char counts as embedded as
    /// well, so the window has to stay well below human typing speed.
    ///
    /// A bare `Enter` drained into a *detected* burst is never routed through
    /// this method: the engine coalesces it as an embedded newline
    /// unconditionally. Reaching the drain loop at all means it arrived
    /// inside the idle-flush window (see [`poll_timeout`](Self::poll_timeout))
    /// that keeps a burst coalescing — i.e. at machine paste speed, which by
    /// definition is not a human `Enter` press. A real submit `Enter`, typed
    /// after the paste settles, arrives past that window: the drain has
    /// already stopped and [`settle`](Self::settle) has already run by the
    /// time it is read, so it starts the *next* batch and is handled there as
    /// an ordinary submit.
    fn enter_is_newline(&self) -> bool;

    /// True while a real paste burst is coalescing — the read loop keeps
    /// draining the event queue instead of processing the batch.
    ///
    /// # Required semantics: latch until [`settle`](Self::settle)
    ///
    /// The engine queries this flag three times for a single burst: once to
    /// decide whether to keep draining events, again after the idle flush to
    /// decide whether to treat the drained batch as a coalesced burst, and once
    /// more after the batch to decide whether to [`settle`](Self::settle). All
    /// must see the same answer, so an implementation MUST latch `true` from the
    /// moment a burst is detected until [`settle`](Self::settle) is called, even
    /// if the burst's inter-char timing has already gone idle by the second
    /// query. Returning `false` once idle (before `settle`) makes the engine
    /// skip the coalescing / [`resolve_burst`](Self::resolve_burst) path for the
    /// already-drained batch, leaking the raw paste text.
    fn is_burst_active(&self) -> bool;

    /// Poll timeout to use while draining an active burst (the idle-flush
    /// window). When a poll of this duration finds no new event, the burst has
    /// settled.
    fn poll_timeout(&self) -> Duration;

    /// Reset detector state after a burst, so the next line starts clean. This
    /// is also the point at which [`is_burst_active`](Self::is_burst_active)
    /// is released from its latch (see that method's required semantics): the
    /// engine calls `settle` once per burst, after the batch holding it has
    /// been processed.
    ///
    /// A batch without a burst is not followed by `settle`. Some event sources
    /// hand the read loop one event per batch, and a reset after each of those
    /// would keep the detector from ever counting up to its threshold. An
    /// implementation must therefore expire stale timing on its own, e.g. by
    /// restarting its count in [`on_char`](Self::on_char) when the gap since
    /// the previous char exceeds its threshold.
    fn settle(&self);

    /// Resolve a settled paste burst. Given the coalesced burst text (embedded
    /// newlines already `\n`), the host may reference-ify it: return `Some(s)` to
    /// have the read loop insert `s` (a `[Pasted text #N, +M lines]` placeholder —
    /// the host stored the original out-of-band) INSTEAD of the raw burst text,
    /// or `None` to keep the raw text. Called at most once per burst batch.
    ///
    /// `coalesced` starts where the burst was detected, which is not always
    /// where the paste began; see the module's limitations.
    fn resolve_burst(&self, coalesced: &str) -> Option<String>;
}
