mod base;
mod circular;
mod default;

pub use base::{Completer, CompletionActionHandler, Span};
pub use circular::CircularCompletionHandler;
pub use default::DefaultCompleter;
