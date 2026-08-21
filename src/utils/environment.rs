use std::ffi::{OsStr, OsString};

/// Read a process environment variable.
///
/// Keeping environment-variable reads in one place gives Reedline a single
/// point for any future synchronization or snapshot policy around process
/// environment access.
pub(crate) fn var_os<K: AsRef<OsStr>>(key: K) -> Option<OsString> {
    std::env::var_os(key)
}
