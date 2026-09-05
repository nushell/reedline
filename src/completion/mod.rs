mod base;
mod default;
pub(crate) mod history;

pub use base::{
    Completer, CompletionOrigin, CompletionResult, CompletionStatus, Partial, Span, Suggestion,
    Suggestions,
};
pub use default::DefaultCompleter;
