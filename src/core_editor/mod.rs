mod clip_buffer;
mod edit_stack;
mod editor;
mod graphemes;
mod line;
mod line_buffer;
#[allow(dead_code)] // wired by the motion resolver
mod word;

#[cfg(feature = "system_clipboard")]
pub(crate) use clip_buffer::get_system_clipboard;
pub(crate) use clip_buffer::{get_local_clipboard, Clipboard};
pub use editor::Editor;
pub use line_buffer::LineBuffer;
