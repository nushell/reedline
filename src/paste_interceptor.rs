//! A generic paste-interception hook.
//!
//! Reedline handles `EditCommand::PasteSystem` (bound to Ctrl+Shift+V by
//! default, under the `system_clipboard` feature) by reading the OS clipboard and inserting the raw text verbatim.
//! Some hosts want to intervene — for example, turn a large or image paste into
//! a compact reference token (`[Pasted text #N, +M lines]`) while stashing the
//! real content elsewhere, then expand it back on submit. This keeps a
//! chat-style / REPL composer readable, a common pattern in modern terminal
//! CLIs. This hook lets a host own that decision: when a `PasteInterceptor` is
//! installed on the [`Reedline`](crate::Reedline) via
//! [`with_paste_interceptor`](crate::Reedline::with_paste_interceptor), an
//! edit that is a lone `PasteSystem` calls [`PasteInterceptor::on_paste`] instead
//! of the default clipboard-read-and-insert, and reedline inserts whatever the
//! hook decides (or nothing).
//!
//! The trait is intentionally generic (no application-specific concepts). The
//! interceptor itself is responsible for reading the clipboard (text and/or
//! image) — reedline does not pass the clipboard content in, because a host that
//! wants to reference-ify a paste typically needs the raw bytes anyway (e.g. an
//! image), which the default text-only `PasteSystem` path never exposes.

/// What reedline should do after a `PasteInterceptor` handles a paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteAction {
    /// Insert this string into the buffer at the cursor (a placeholder token,
    /// or the raw pasted text if the host chose not to reference-ify it).
    InsertText(String),
    /// Insert nothing (e.g. empty clipboard, a read error, or the host stored
    /// the content out-of-band and wants no visible buffer change).
    Noop,
}

/// A host hook invoked when an edit consisting of a lone
/// `EditCommand::PasteSystem` fires and an interceptor is installed.
/// Implementations read the clipboard themselves and return the
/// [`PasteAction`] reedline should apply.
///
/// Key events of one batch are fused into one edit, so a `PasteSystem` that
/// shares its batch with other edits (a key typed right before it) takes the
/// default path instead.
///
/// Must be `Send + Sync` because it is held behind an `Arc` on the `Reedline`
/// engine, which is moved across the read loop.
pub trait PasteInterceptor: Send + Sync {
    /// Called on an intercepted paste. The implementation reads the clipboard and
    /// decides what (if anything) to insert into the line buffer.
    fn on_paste(&self) -> PasteAction;

    /// Optionally rewrite the buffer at submit — e.g. expand compact
    /// paste-reference placeholders to their full text (the compact form is
    /// only for composing).
    ///
    /// The returned string REPLACES the line: it is what the final repaint
    /// shows, what [`Signal::Success`](crate::Signal::Success) hands back,
    /// and what goes into history. Return `None` to leave the buffer unchanged
    /// (the default). Must be cheap and side-effect-free w.r.t. any
    /// out-of-band store (read-only): the host's own submit path still
    /// consumes the store.
    ///
    /// It runs once the line is accepted, so a [`Validator`](crate::Validator)
    /// has judged the unexpanded buffer, while the history exclusion prefix is
    /// checked against the expansion.
    fn expand_on_submit(&self, _buffer: &str) -> Option<String> {
        None
    }
}
