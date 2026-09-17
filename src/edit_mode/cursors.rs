use crossterm::cursor::SetCursorStyle;

/// Maps cursor shapes to each edit mode (emacs, vi and helix).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
///
/// Build one with `..CursorConfig::default()` rather than an exhaustive
/// literal, so a mode gaining a field of its own does not break the literal.
#[derive(Default)]
pub struct CursorConfig {
    /// The cursor to be used when in vi insert mode
    pub vi_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in vi normal mode
    pub vi_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in emacs mode
    pub emacs: Option<SetCursorStyle>,
    /// The cursor to be used when in hx insert mode
    pub hx_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in hx normal mode
    pub hx_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in hx select mode
    pub hx_select: Option<SetCursorStyle>,
}
