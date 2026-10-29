/// Text drawn on its own rows below the buffer: the host's commentary on the
/// line being edited, such as the first error in it, the signature of the
/// command under the cursor, or the type the line evaluates to.
///
/// zsh calls the same area the status line (`zle -R`). It sits between the
/// buffer and an open menu, so both stay visible, and it is erased before the
/// line is submitted, like a hint. It stays while Up and Down walk through
/// history, describing each line they recall, and is hidden in history search.
///
/// A buffer too tall for the screen to also hold the status line draws
/// without it, as does a menu that cannot shrink to leave it room.
///
/// ```rust
/// use reedline::{Reedline, StatusLine};
///
/// struct Length;
///
/// impl StatusLine for Length {
///     fn status(&mut self, line: &str, _pos: usize, _columns: u16, _color: bool) -> String {
///         format!("{} bytes", line.len())
///     }
/// }
///
/// let line_editor = Reedline::create().with_status_line(Box::new(Length));
/// ```
pub trait StatusLine: Send {
    /// The status for `line` with the cursor at byte offset `pos`, on a
    /// terminal `columns` wide. An empty string draws no status line.
    ///
    /// The text may hold ANSI styling (SGR) when `use_ansi_coloring` is set,
    /// and wraps like any other text when it is wider than `columns`. Trailing
    /// line breaks are dropped, tabs are expanded to stops every 8 columns,
    /// and other control characters show escaped (`^[`, `^M`), as in the
    /// buffer. It is asked for on every repaint with a row free below the
    /// buffer, so cache by `line` if producing it is expensive.
    fn status(&mut self, line: &str, pos: usize, columns: u16, use_ansi_coloring: bool) -> String;
}
