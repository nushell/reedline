/// The syntax validation trait. Implementers of this trait will check to see if the current input
/// is incomplete and spans multiple lines
pub trait Validator {
    /// The action that will handle the current buffer as a line and return the corresponding validation
    fn validate(&self, line: &str) -> ValidationResult;
}

/// Whether or not the validation shows the input was complete
pub enum ValidationResult {
    /// An incomplete input which may need to span multiple lines to be complete
    Incomplete,

    /// An input that is complete as-is
    Complete,
}

/// A default validator which checks for mismatched quotes
pub struct DefaultValidator;

impl Validator for DefaultValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        if line.split('"').count() % 2 == 0 || incomplete_brackets(line) {
            ValidationResult::Incomplete
        } else {
            ValidationResult::Complete
        }
    }
}

fn incomplete_brackets(line: &str) -> bool {
    let mut brackets = 0;
    let mut square_brackets = 0;
    let mut pharentesis = 0;

    for c in line.chars() {
        if c == '{' {
            brackets += 1;
        } else if c == '}' {
            brackets -= 1;
        } else if c == '[' {
            square_brackets += 1
        } else if c == ']' {
            square_brackets -= 1
        } else if c == '(' {
            pharentesis += 1
        } else if c == ')' {
            pharentesis -= 1
        }
    }

    !(brackets == 0 && square_brackets == 0 && pharentesis == 0)
}
