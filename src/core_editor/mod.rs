mod clip_buffer;
mod cursor;
mod edit_stack;
mod editor;
mod graphemes;
mod line;
mod line_buffer;
mod resolve;
mod rest_policy;
mod word;

#[cfg(feature = "system_clipboard")]
pub(crate) use clip_buffer::get_system_clipboard;
pub(crate) use clip_buffer::{get_local_clipboard, Clipboard};
pub(crate) use cursor::{CaretGeometry, Cursor, Movement, SelectionExtent};
pub use editor::Editor;
pub use line_buffer::LineBuffer;
pub(crate) use resolve::{operator_span, resolve_motion};
pub(crate) use rest_policy::{commit, RestPolicy};
