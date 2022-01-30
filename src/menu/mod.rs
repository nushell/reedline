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
    /// This function needs to be defined in the trait because when the menu is
    /// activated the len of the values is calculated to know if there is only one
    /// value so it can be selected immediately
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    );

    /// The working details of a menu are values that could change based on
    /// the menu conditions before it being printed, such as the number or size
    /// of columns, etc.
    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    );

    /// Indicates how to replace in the buffer the selected value from the menu
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer);

    /// Text style for the values printed in the menu. The index represents the
    /// selected values in the menu
    fn text_style(&self, index: usize) -> String;

    /// Moves the selected value to the next element
    fn edit_line_buffer(&mut self);

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

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16;

    /// Row position
    fn row_pos(&self) -> u16;

    /// Column position
    fn col_pos(&self) -> u16;

    /// Gets cached values from menu that will be displayed
    fn get_values(&self) -> &[(Span, String)];

    /// Returns working details columns
    fn get_cols(&self) -> u16;

    /// Returns working details col width
    fn get_width(&self) -> usize;

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal or if entries have multiple lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16;

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String;

    /// Get selected value from the menu
    fn get_value(&self) -> Option<(Span, String)> {
        self.get_values().get(self.position()).cloned()
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
}
