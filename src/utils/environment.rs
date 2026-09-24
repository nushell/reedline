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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_dumb_is_dumb() {
        assert!(term_is_dumb(Some(OsStr::new("dumb"))));
    }

    #[test]
    fn regular_term_is_not_dumb() {
        assert!(!term_is_dumb(Some(OsStr::new("xterm-256color"))));
    }

    #[test]
    fn unset_term_is_not_dumb() {
        assert!(!term_is_dumb(None));
    }
}
