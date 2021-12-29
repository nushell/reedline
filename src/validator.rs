/// The syntax validation trait. Implementers of this trait will check to see if the current input
/// is incomplete and spans multiple lines
pub trait Validator: Send + Sync {
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
    let mut balance: Vec<char> = Vec::new();

    for c in line.chars() {
        if c == '{' {
            balance.push('}');
        } else if c == '[' {
            balance.push(']');
        } else if c == '(' {
            balance.push(')');
        } else if ['}', ']', ')'].contains(&c) {
            if let Some(last) = balance.last() {
                if last == &c {
                    balance.pop();
                }
            }
        }
    }

    !balance.is_empty()
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("(([[]]))", false)]
    #[case("(([[]]", true)]
    #[case("{[}]", true)]
    #[case("{[]}{()}", false)]
    fn test_incomplete_brackets(#[case] input: &str, #[case] expected: bool) {
        let result = incomplete_brackets(input);

        assert_eq!(result, expected);
    }
}
