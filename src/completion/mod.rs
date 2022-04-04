mod base;
mod circular;
mod default;
pub(crate) mod history;

pub use base::{Completer, Span, Suggestion};
pub use circular::CircularCompletionHandler;
pub use default::DefaultCompleter;
