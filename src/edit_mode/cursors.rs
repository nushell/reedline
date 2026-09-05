use crossterm::cursor::SetCursorStyle;

/// Maps cursor shapes to each edit mode (emacs, vi and, under the `helix`
/// feature, helix).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
///
/// The `hx_*` fields only exist with the `helix` feature, so build one with
/// `..CursorConfig::default()` rather than an exhaustive literal; otherwise the
/// set of fields a literal must name changes with the feature set.
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
