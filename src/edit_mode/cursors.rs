use crossterm::cursor::SetCursorStyle;

/// Maps cursor shapes to each edit mode (emacs, vi normal & vi insert).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
#[derive(Default)]
pub struct CursorConfig {
    /// The cursor to be used when in vi insert mode
    pub vi_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in vi normal mode
    pub vi_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in emacs mode
    pub emacs: Option<SetCursorStyle>,
}
