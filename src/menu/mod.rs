mod completion_menu;
mod history_menu;

use crate::{painting::Painter, Completer, History, LineBuffer, Suggestion};
pub use completion_menu::CompletionMenu;
pub use history_menu::HistoryMenu;
use nu_ansi_term::{Color, Style};

/// Struct to store the menu style
struct MenuTextStyle {
    selected_text_style: Style,
    text_style: Style,
    description_style: Style,
}

impl Default for MenuTextStyle {
    fn default() -> Self {
        Self {
            selected_text_style: Color::Green.bold().reverse(),
            text_style: Color::DarkGray.normal(),
            description_style: Color::Yellow.normal(),
        }
    }
}

/// Defines all possible events that could happen with a menu.
#[derive(Clone)]
pub enum MenuEvent {
    /// Activation event for the menu. When the bool is true it means that the values
    /// have already being updated. This is true when the option `quick_completions` is true
    Activate(bool),
    /// Deactivation event
    Deactivate,
    /// Line buffer edit event. When the bool is true it means that the values
    /// have already being updated. This is true when the option `quick_completions` is true
    Edit(bool),
    /// Selecting next element in the menu
    NextElement,
    /// Selecting previous element in the menu
    PreviousElement,
    /// Moving up in the menu
    MoveUp,
    /// Moving down in the menu
    MoveDown,
    /// Moving left in the menu
    MoveLeft,
    /// Moving right in the menu
    MoveRight,
    /// Move to next page
    NextPage,
    /// Move to previous page
    PreviousPage,
}

/// Trait that defines how a menu will be printed by the painter
pub trait Menu: Send {
    /// Menu name
    fn name(&self) -> &str;

    /// Menu indicator
    fn indicator(&self) -> &str {
        "% "
    }

    /// Checks if the menu is active
    fn is_active(&self) -> bool;

    /// Selects what type of event happened with the menu
    fn menu_event(&mut self, event: MenuEvent);

    /// The completion menu can try to find the common string and replace it
    /// in the given line buffer
    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    ) -> bool;

    /// Updates the values presented in the menu
    /// This function needs to be defined in the trait because when the menu is
    /// activated or the `quick_completion` option is true, the len of the values
    /// is calculated to know if there is only one value so it can be selected
    /// immediately
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    );

    /// The working details of a menu are values that could change based on
    /// the menu conditions before it being printed, such as the number or size
    /// of columns, etc.
    /// In this function should be defined how the menu event is treated since
    /// it is called just before painting the menu
    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    );

    /// Indicates how to replace in the line buffer the selected value from the menu
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer);

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal or if entries have multiple lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16;

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String;

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16;

    /// Gets cached values from menu that will be displayed
    fn get_values(&self) -> &[Suggestion];
}

pub(crate) enum IndexDirection {
    Forward,
    Backward,
}

pub(crate) struct ParseResult<'buffer> {
    pub remainder: &'buffer str,
    pub index: Option<usize>,
    pub marker: Option<&'buffer str>,
    pub direction: IndexDirection,
}

/// Splits a string that contains a marker character
/// e.g: this is an example!10
///     returns:
///         this is an example
///         (10, "!10") (index and index as string)
pub(crate) fn parse_selection_char(buffer: &str, marker: char) -> ParseResult {
    if buffer.is_empty() {
        return ParseResult {
            remainder: buffer,
            index: None,
            marker: None,
            direction: IndexDirection::Forward,
        };
    }

    let mut input = buffer.chars().peekable();

    let mut index = 0;
    let mut direction = IndexDirection::Forward;
    while let Some(char) = input.next() {
        if char == marker {
            match input.peek() {
                Some(&x) if x == marker => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..index + 2]),
                        direction: IndexDirection::Backward,
                    }
                }
                Some(&x) if x.is_ascii_digit() || x == '-' => {
                    let mut count: usize = 0;
                    let mut size: usize = 1;
                    while let Some(&c) = input.peek() {
                        if c == '-' {
                            let _ = input.next();
                            size += 1;
                            direction = IndexDirection::Backward;
                        } else if c.is_ascii_digit() {
                            let c = c.to_digit(10).expect("already checked if is a digit");
                            let _ = input.next();
                            count *= 10;
                            count += c as usize;
                            size += 1;
                        } else {
                            return ParseResult {
                                remainder: &buffer[0..index],
                                index: Some(count),
                                marker: Some(&buffer[index..index + size]),
                                direction,
                            };
                        }
                    }
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(count),
                        marker: Some(&buffer[index..index + size]),
                        direction,
                    };
                }
                None => {
                    return ParseResult {
                        remainder: &buffer[0..index],
                        index: Some(0),
                        marker: Some(&buffer[index..buffer.len()]),
                        direction,
                    }
                }
                _ => {
                    index += 1;
                    continue;
                }
            }
        }
        index += 1;
    }

    ParseResult {
        remainder: buffer,
        index: None,
        marker: None,
        direction,
    }
}

/// Finds index for the common string in the suggestions
fn find_common_string(values: &[Suggestion]) -> (Option<&Suggestion>, Option<usize>) {
    let first = values.iter().next();

    let index = first.and_then(|first| {
        values.iter().skip(1).fold(None, |index, suggestion| {
            if suggestion.value.starts_with(&first.value) {
                Some(first.value.len())
            } else {
                first
                    .value
                    .chars()
                    .zip(suggestion.value.chars())
                    .position(|(mut lhs, mut rhs)| {
                        lhs.make_ascii_lowercase();
                        rhs.make_ascii_lowercase();

                        lhs != rhs
                    })
                    .map(|new_index| match index {
                        Some(index) => {
                            if index <= new_index {
                                index
                            } else {
                                new_index
                            }
                        }
                        None => new_index,
                    })
            }
        })
    });

    (first, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_row_test() {
        let input = "search:6";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(6));
        assert_eq!(res.marker, Some(":6"));
    }

    #[test]
    fn parse_double_char() {
        let input = "search!!";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some("!!"));
        assert!(matches!(res.direction, IndexDirection::Backward));
    }

    #[test]
    fn parse_row_other_marker_test() {
        let input = "search?9";
        let res = parse_selection_char(input, '?');

        assert_eq!(res.remainder, "search");
        assert_eq!(res.index, Some(9));
        assert_eq!(res.marker, Some("?9"));
    }

    #[test]
    fn parse_row_double_test() {
        let input = "ls | where:16";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "ls | where");
        assert_eq!(res.index, Some(16));
        assert_eq!(res.marker, Some(":16"));
    }

    #[test]
    fn parse_row_empty_test() {
        let input = ":10";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, Some(10));
        assert_eq!(res.marker, Some(":10"));
    }

    #[test]
    fn parse_row_fake_indicator_test() {
        let input = "let a: another :10";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "let a: another ");
        assert_eq!(res.index, Some(10));
        assert_eq!(res.marker, Some(":10"));
    }

    #[test]
    fn parse_row_no_number_test() {
        let input = "let a: another:";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "let a: another");
        assert_eq!(res.index, Some(0));
        assert_eq!(res.marker, Some(":"));
    }

    #[test]
    fn parse_empty_buffer_test() {
        let input = "";
        let res = parse_selection_char(input, ':');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, None);
        assert_eq!(res.marker, None);
    }

    #[test]
    fn parse_negative_direction() {
        let input = "!-2";
        let res = parse_selection_char(input, '!');

        assert_eq!(res.remainder, "");
        assert_eq!(res.index, Some(2));
        assert_eq!(res.marker, Some("!-2"));
        assert!(matches!(res.direction, IndexDirection::Backward));
    }
}
