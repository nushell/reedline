mod context_menu;
mod history_menu;

use crate::Span;
pub use context_menu::{ContextMenu, ContextMenuInput};
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
pub trait Menu {
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
