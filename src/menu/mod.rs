mod context_menu;
mod history_menu;

use crate::{Completer, History, LineBuffer, Span};
pub use context_menu::ContextMenu;
pub use history_menu::HistoryMenu;
use nu_ansi_term::{ansi::RESET, Color, Style};

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

    /// Activates the menu
    fn activate(&mut self);

    /// Deactivates the menu
    fn deactivate(&mut self);

    /// Updates the values presented in the menu
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    );

    /// Updates working details of the menu
    fn update_working_details(&mut self, _screen_width: u16) {}

    /// Moves the selected value to the next element
    fn move_next(&mut self);

    /// Moves the selected value to the previous element
    fn move_previous(&mut self);

    /// Moves the selected value up
    fn move_up(&mut self) {}

    /// Moves the selected value down
    fn move_down(&mut self) {}

    /// Moves the selected value left
    fn move_left(&mut self) {}

    /// Moves the selected value right
    fn move_right(&mut self) {}

    /// If the menu is paginated then it shows the next page
    fn next_page(&mut self) {}

    /// If the menu is paginated then it shows the previous page
    fn previous_page(&mut self) {}

    /// Replace in buffer
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer);

    /// Text style for menu
    fn text_style(&self, index: usize) -> String;

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16;

    /// Row position
    fn row_pos(&self) -> u16;

    /// Column position
    fn col_pos(&self) -> u16;

    /// Gets values from filler that will be displayed in the menu
    fn get_values(&self) -> &[(Span, String)];

    /// Returns working details columns
    fn get_cols(&self) -> u16;

    /// Returns working details col width
    fn get_width(&self) -> usize;

    /// Get selected value from filler
    fn get_value(&self) -> Option<(Span, String)> {
        self.get_values().get(self.position()).cloned()
    }

    /// Get number of values
    fn get_num_values(&self) -> usize {
        self.get_values().len()
    }

    /// Calculates how many rows the Menu will use
    fn get_rows(&self) -> u16 {
        let rows = self.get_values().len() as u16 / self.get_cols();
        rows + 1
    }

    /// Menu index based on column and row position
    fn position(&self) -> usize {
        let position = self.row_pos() * self.get_cols() + self.col_pos();
        position as usize
    }

    /// End of line for menu
    fn end_of_line(&self, column: u16) -> &str {
        if column == self.get_cols().saturating_sub(1) {
            "\r\n"
        } else {
            ""
        }
    }

    /// Print enumerate
    fn print_enumerate(&self) -> bool {
        false
    }

    /// Characters that will be shown when an element of the menu has multiple lines
    fn multiline_marker(&self) -> &str {
        ":::"
    }

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, remaining_lines: u16, use_ansi_coloring: bool) -> String {
        // The skip values represent the number of lines that should be skipped
        // while printing the menu
        let skip_values = if self.row_pos() >= remaining_lines {
            let skip_lines = self.row_pos().saturating_sub(remaining_lines) + 1;
            (skip_lines * self.get_cols()) as usize
        } else {
            0
        };

        // It seems that crossterm prefers to have a complete string ready to be printed
        // rather than looping through the values and printing multiple things
        // This reduces the flickering when printing the menu
        let available_values = (remaining_lines * self.get_cols()) as usize;
        self.get_values()
            .iter()
            .skip(skip_values)
            .take(available_values)
            .enumerate()
            .map(|(index, (_, line))| {
                // Correcting the enumerate index based on the number of skipped values
                let index = index + skip_values;
                let column = index as u16 % self.get_cols();
                let empty_space = self.get_width().saturating_sub(line.len());

                // Final string with colors
                let line = line.replace("\n", format!("\n{}", self.multiline_marker()).as_str());
                let row_number = if self.print_enumerate() {
                    format!("{}: ", index)
                } else {
                    "".to_string()
                };

                if use_ansi_coloring {
                    format!(
                        "{}{}{}{}{:empty$}{}",
                        row_number,
                        self.text_style(index),
                        &line,
                        RESET,
                        "",
                        self.end_of_line(column),
                        empty = empty_space
                    )
                } else {
                    // If no ansi coloring is found, then the selection word is
                    // the line in uppercase
                    let line_str = if index == self.position() {
                        format!("{}>{}", row_number, line.to_uppercase())
                    } else {
                        format!("{}{}", row_number, line)
                    };

                    // Final string with formatting
                    format!(
                        "{:width$}{}",
                        line_str,
                        self.end_of_line(column),
                        width = self.get_width()
                    )
                }
            })
            .collect()
    }
}

fn parse_row_selector<'buffer>(
    buffer: &'buffer str,
    marker: &char,
) -> (&'buffer str, Option<usize>) {
    let mut input = buffer.chars().peekable();

    let mut index = 0;
    while let Some(char) = input.next() {
        if &char == marker {
            match input.peek() {
                Some(x) if x.is_ascii_digit() => {
                    let mut count: usize = 0;
                    while let Some(&c) = input.peek() {
                        if c.is_ascii_digit() {
                            let c = c.to_digit(10).expect("already checked if is a digit");
                            let _ = input.next();
                            count *= 10;
                            count += c as usize;
                        } else {
                            return (&buffer[0..index], Some(count));
                        }
                    }
                    return (&buffer[0..index], Some(count));
                }
                None => {
                    return (&buffer[0..index], None);
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
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "search");
        assert_eq!(row, Some(6))
    }

    #[test]
    fn parse_row_other_marker_test() {
        let input = "search?9";
        let (res, row) = parse_row_selector(input, &'?');

        assert_eq!(res, "search");
        assert_eq!(row, Some(9))
    }

    #[test]
    fn parse_row_double_test() {
        let input = "ls | where:16";
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "ls | where");
        assert_eq!(row, Some(16))
    }

    #[test]
    fn parse_row_empty_test() {
        let input = ":10";
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "");
        assert_eq!(row, Some(10))
    }

    #[test]
    fn parse_row_fake_indicator_test() {
        let input = "let a: another :10";
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "let a: another ");
        assert_eq!(row, Some(10))
    }

    #[test]
    fn parse_row_no_number_test() {
        let input = "let a: another:";
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "let a: another");
        assert_eq!(row, None)
    }
}
