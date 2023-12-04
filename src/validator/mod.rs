mod default;
pub use default::DefaultValidator;

/// The syntax validation trait. Implementers of this trait will check to see if the current input
/// is incomplete and spans multiple lines
pub trait Validator: Send {
    /// The action that will handle the current buffer as a line and return the corresponding validation
    fn validate(&self, line: &str) -> ValidationResult;
}

#[derive(Clone, Copy)]
/// Whether or not the validation shows the input was complete
pub enum ValidationResult {
    /// An incomplete input which may need to span multiple lines to be complete
    Incomplete,

    /// An input that is complete as-is
    Complete,
}
