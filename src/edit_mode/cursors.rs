use crossterm::cursor::CursorShape;

/// Maps cursor shapes to each edit mode (emacs, vi normal & vi insert).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
pub struct CursorConfig {
    /// The cursor to be used when in vi insert mode
    pub vi_insert: Option<CursorShape>,
    /// The cursor to be used when in vi normal mode
    pub vi_normal: Option<CursorShape>,
    /// The cursor to be used when in emacs mode
    pub emacs: Option<CursorShape>,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            vi_insert: None,
            vi_normal: None,
            emacs: None,
        }
    }
}
