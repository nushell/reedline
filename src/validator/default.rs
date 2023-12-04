use crate::{ValidationResult, Validator};

/// A default validator which checks for mismatched quotes and brackets
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
