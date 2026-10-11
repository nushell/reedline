use crate::{PromptEditMode, PromptHelixMode, PromptViMode};
use crossterm::cursor::SetCursorStyle;

/// Maps cursor shapes to each edit mode (emacs, vi and helix).
/// If any of the fields is `None`, the cursor won't get changed by Reedline for that mode.
///
/// A host that names only some of the fields should build it with
/// `..CursorConfig::default()`, so a mode gaining a slot of its own does not
/// break the literal. One that names every field, like the demo, needs no
/// spread.
#[derive(Default)]
pub struct CursorConfig {
    /// The cursor to be used when in vi insert mode
    pub vi_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in vi normal mode
    pub vi_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in vi visual mode. `None` follows
    /// [`vi_normal`](Self::vi_normal) instead of leaving the cursor alone:
    /// visual drew the normal shape before it had a slot, and a config that
    /// only names `vi_normal` keeps that.
    pub vi_visual: Option<SetCursorStyle>,
    /// The cursor to be used when in emacs mode
    pub emacs: Option<SetCursorStyle>,
    /// The cursor to be used when in hx insert mode
    pub hx_insert: Option<SetCursorStyle>,
    /// The cursor to be used when in hx normal mode
    pub hx_normal: Option<SetCursorStyle>,
    /// The cursor to be used when in hx select mode
    pub hx_select: Option<SetCursorStyle>,
}

impl CursorConfig {
    /// The shape to draw in `mode`, `None` to leave the cursor as it is.
    pub(crate) fn shape_for(&self, mode: &PromptEditMode) -> Option<SetCursorStyle> {
        match mode {
            PromptEditMode::Emacs => self.emacs,
            PromptEditMode::Vi(PromptViMode::Insert) => self.vi_insert,
            PromptEditMode::Vi(PromptViMode::Normal) => self.vi_normal,
            PromptEditMode::Vi(PromptViMode::Visual) => self.vi_visual.or(self.vi_normal),
            PromptEditMode::Helix(PromptHelixMode::Insert) => self.hx_insert,
            PromptEditMode::Helix(PromptHelixMode::Normal) => self.hx_normal,
            PromptEditMode::Helix(PromptHelixMode::Select) => self.hx_select,
            PromptEditMode::Default | PromptEditMode::Custom(_) => None,
        }
    }
}
