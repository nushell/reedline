mod base;
mod circular;
mod default;

pub use base::{Completer, Span, Suggestion};
pub use circular::CircularCompletionHandler;
pub use default::DefaultCompleter;
