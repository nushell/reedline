mod clip_buffer;
mod cursor;
mod edit_stack;
mod editor;
mod graphemes;
mod line;
mod line_buffer;
#[allow(dead_code)] // wired when the editor lowers the operator verbs
mod resolve;
mod rest_policy;
mod word;

#[cfg(feature = "system_clipboard")]
pub(crate) use clip_buffer::get_system_clipboard;
pub(crate) use clip_buffer::{get_local_clipboard, Clipboard};
pub(crate) use cursor::Cursor;
pub use editor::Editor;
pub use line_buffer::LineBuffer;
pub(crate) use rest_policy::{commit, RestPolicy};
