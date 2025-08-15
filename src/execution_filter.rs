/// Execution filtering for command delegation
#[cfg(feature = "execution_filter")]
use std::fmt::Debug;

/// Decision on how to execute a command
#[cfg(feature = "execution_filter")]
#[derive(Debug)]
pub enum FilterDecision {
    /// Execute the command normally in the REPL
    Execute(String),
    /// Delegate the command to an external handler
    Delegate(String),
}

/// Trait for filtering command execution
///
/// This allows REPL applications to intercept commands and decide
/// whether to execute them normally or delegate to an external handler.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "execution_filter")]
/// # {
/// use reedline::{ExecutionFilter, FilterDecision};
///
/// struct PtyFilter;
///
/// impl ExecutionFilter for PtyFilter {
///     fn filter(&self, command: &str) -> FilterDecision {
///         // Check if command needs special handling
///         let cmd = command.split_whitespace().next().unwrap_or("");
///         if matches!(cmd, "vim" | "ssh" | "nano" | "htop") {
///             FilterDecision::Delegate(command.to_string())
///         } else {
///             FilterDecision::Execute(command.to_string())
///         }
///     }
/// }
/// # }
/// ```
#[cfg(feature = "execution_filter")]
pub trait ExecutionFilter: Send + Sync + Debug {
    /// Decide how to execute the given command
    fn filter(&self, command: &str) -> FilterDecision;
}
