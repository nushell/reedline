mod base;
mod default;
pub(crate) mod history;

pub use base::{Completer, Span, Suggestion};
pub use default::DefaultCompleter;
