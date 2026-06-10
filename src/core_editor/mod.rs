mod clip_buffer;
mod cursor;
mod edit_stack;
mod editor;
mod graphemes;
mod line;
mod line_buffer;
mod rest_policy;
#[allow(dead_code)] // wired by the motion resolver
mod word;

#[cfg(feature = "system_clipboard")]
pub(crate) use clip_buffer::get_system_clipboard;
pub(crate) use clip_buffer::{get_local_clipboard, Clipboard};
pub(crate) use cursor::Cursor;
pub use editor::Editor;
pub use line_buffer::LineBuffer;
pub(crate) use rest_policy::{commit, RestPolicy};
