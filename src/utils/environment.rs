use std::ffi::{OsStr, OsString};

/// Read a process environment variable.
///
/// Keeping environment-variable reads in one place gives Reedline a single
/// point for any future synchronization or snapshot policy around process
/// environment access.
pub(crate) fn var_os<K: AsRef<OsStr>>(key: K) -> Option<OsString> {
    std::env::var_os(key)
}

/// Whether the terminal explicitly declares itself as lacking terminal capabilities.
pub(crate) fn term_is_dumb(term: Option<&OsStr>) -> bool {
    term == Some(OsStr::new("dumb"))
}

/// Whether ANSI coloring is appropriate for the declared terminal.
///
/// An unset or non-dumb `TERM` preserves the configured Reedline behavior.
pub(crate) fn term_supports_ansi(term: Option<&OsStr>) -> bool {
    !term_is_dumb(term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_dumb_does_not_support_ansi() {
        assert!(!term_supports_ansi(Some(OsStr::new("dumb"))));
    }

    #[test]
    fn regular_term_supports_ansi() {
        assert!(term_supports_ansi(Some(OsStr::new("xterm-256color"))));
    }

    #[test]
    fn unset_term_does_not_disable_ansi() {
        assert!(term_supports_ansi(None));
    }
}
