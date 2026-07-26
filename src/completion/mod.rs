mod base;
mod default;
pub(crate) mod history;

pub use base::{Completer, CompletionResult, CompletionStatus, Span, Suggestion, Suggestions};
pub use default::DefaultCompleter;
