mod clip_buffer;
mod edit_stack;
mod editor;
mod line_buffer;

#[cfg(feature = "system_clipboard")]
pub(crate) use clip_buffer::get_system_clipboard;
pub(crate) use clip_buffer::{get_local_clipboard, Clipboard, ClipboardMode};
pub use editor::Editor;
pub use line_buffer::LineBuffer;
