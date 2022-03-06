mod completion_menu;
mod history_menu;

use crate::{painter::Painter, Completer, History, LineBuffer, Span};
pub use completion_menu::CompletionMenu;
pub use history_menu::HistoryMenu;
use nu_ansi_term::{Color, Style};

/// Struct to store the menu style
struct MenuTextStyle {
    selected_text_style: Style,
    text_style: Style,
}

impl Default for MenuTextStyle {
    fn default() -> Self {
        Self {
            selected_text_style: Color::Green.bold().reverse(),
            text_style: Color::DarkGray.normal(),
        }
    }
}

/// Defines all possible events that could happen with a menu.
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

    /// Updates the values presented in the menu
    /// This function needs to be defined in the trait because when the menu is
    /// activated or the quick_completion option is true, the len of the values
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
    fn get_values(&self) -> &[(Span, String)];
}

/// Splits a string that contains a marker character
/// e.g: this is an example!10
///     returns:
///         this is an example
///         (10, "!10") (index and index as string)
pub(crate) fn parse_selection_char<'buffer>(
    buffer: &'buffer str,
    marker: &char,
) -> (&'buffer str, Option<(usize, &'buffer str)>) {
    if buffer.is_empty() {
        return (buffer, None);
    }

    let mut input = buffer.chars().peekable();

    let mut index = 0;
    while let Some(char) = input.next() {
        if &char == marker {
            match input.peek() {
                Some(x) if x == marker => {
                    return (&buffer[0..index], Some((0, &buffer[index..index + 2])));
                }
                Some(x) if x.is_ascii_digit() => {
                    let mut count: usize = 0;
                    let mut size: usize = 1;
                    while let Some(&c) = input.peek() {
                        if c.is_ascii_digit() {
                            let c = c.to_digit(10).expect("already checked if is a digit");
                            let _ = input.next();
                            count *= 10;
                            count += c as usize;
                            size += 1;
                        } else {
                            return (
                                &buffer[0..index],
                                Some((count, &buffer[index..index + size])),
                            );
                        }
                    }
                    return (
                        &buffer[0..index],
                        Some((count, &buffer[index..index + size])),
                    );
                }
                None => {
                    return (&buffer[0..index], Some((0, &buffer[index..buffer.len()])));
                }
                _ => {
                    index += 1;
                    continue;
                }
            }
        }
        index += 1
    }

    (buffer, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_row_test() {
        let input = "search:6";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "search");
        assert_eq!(row, Some((6, ":6")))
    }

    #[test]
    fn parse_double_char() {
        let input = "search!!";
        let (res, row) = parse_selection_char(input, &'!');

        assert_eq!(res, "search");
        assert_eq!(row, Some((0, "!!")))
    }

    #[test]
    fn parse_row_other_marker_test() {
        let input = "search?9";
        let (res, row) = parse_selection_char(input, &'?');

        assert_eq!(res, "search");
        assert_eq!(row, Some((9, "?9")))
    }

    #[test]
    fn parse_row_double_test() {
        let input = "ls | where:16";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "ls | where");
        assert_eq!(row, Some((16, ":16")))
    }

    #[test]
    fn parse_row_empty_test() {
        let input = ":10";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "");
        assert_eq!(row, Some((10, ":10")))
    }

    #[test]
    fn parse_row_fake_indicator_test() {
        let input = "let a: another :10";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "let a: another ");
        assert_eq!(row, Some((10, ":10")))
    }

    #[test]
    fn parse_row_no_number_test() {
        let input = "let a: another:";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "let a: another");
        assert_eq!(row, Some((0, ":")))
    }

    #[test]
    fn parse_empty_buffer_test() {
        let input = "";
        let (res, row) = parse_selection_char(input, &':');

        assert_eq!(res, "");
        assert_eq!(row, None)
    }
}
