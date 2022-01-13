use nu_ansi_term::Color;

use crate::{Completer, DefaultCompleter, LineBuffer, Span};

/// Struct to store coloring for the menu
struct MenuTextColor {
    selection_style: String,
    text_style: String,
}

impl Default for MenuTextColor {
    fn default() -> Self {
        Self {
            selection_style: Color::Green.bold().reverse().prefix().to_string(),
            text_style: Color::DarkGray.prefix().to_string(),
        }
    }
}

pub struct ColumnDetails {
    /// Number of columns that the menu will have
    pub cols: u16,
    /// Column width
    pub col_width: usize,
    /// Column padding
    pub col_padding: usize,
}

impl ColumnDetails {
    fn new(cols: u16, col_width: usize, col_padding: usize) -> Self {
        Self {
            cols,
            col_width,
            col_padding,
        }
    }
}

/// Context menu definition
pub struct ContextMenu {
    completer: Box<dyn Completer>,
    active: bool,
    /// Context menu coloring
    color: MenuTextColor,
    /// Number of minimum rows that are displayed when
    /// the required lines is larger than the available lines
    min_rows: u16,
    /// column position of the cursor. Starts from 0
    pub col_pos: u16,
    /// row position in the menu. Starts from 0
    pub row_pos: u16,
    /// Default column details that are set when creating the menu
    default_details: ColumnDetails,
    /// Working column details keep changing based on the collected values
    working_details: ColumnDetails,
}

impl Default for ContextMenu {
    fn default() -> Self {
        let completer = Box::new(DefaultCompleter::default());
        Self::new_with(completer)
    }
}

impl ContextMenu {
    /// Creates a context menu with a filler
    pub fn new_with(completer: Box<dyn Completer>) -> Self {
        Self {
            completer,
            active: false,
            color: MenuTextColor::default(),
            min_rows: 3,
            col_pos: 0,
            row_pos: 0,
            default_details: ColumnDetails::new(4, 20, 2),
            working_details: ColumnDetails::new(4, 20, 2),
        }
    }

    /// Activates context menu
    pub fn activate(&mut self, line_buffer: &LineBuffer, screen_width: u16) {
        self.active = true;
        self.reset_position();

        let max_width = self
            .get_values(line_buffer)
            .iter()
            .fold(0, |acc, (_, string)| {
                let str_len = string.len() + self.working_details.col_padding;
                if str_len > acc {
                    str_len
                } else {
                    acc
                }
            });

        // Adjusting the working width of the column based the max line width found
        // in the menu values
        if max_width > self.default_details.col_width {
            self.working_details.col_width = max_width;
        } else {
            self.working_details.col_width = self.default_details.col_width;
        }

        // The working columns is adjusted based on possible number of columns
        // that could be fitted in the screen with the calculated column width
        let possible_cols = screen_width / self.working_details.col_width as u16;
        if possible_cols > self.default_details.cols {
            self.working_details.cols = self.default_details.cols;
        } else {
            self.working_details.cols = possible_cols;
        }
    }

    /// Deactivates context menu
    pub fn deactivate(&mut self) {
        self.active = false
    }

    /// Deactivates context menu
    pub fn is_active(&mut self) -> bool {
        self.active
    }

    /// Gets values from filler that will be displayed in the menu
    pub fn get_values(&self, line_buffer: &LineBuffer) -> Vec<(Span, String)> {
        self.completer
            .complete(line_buffer.get_buffer(), line_buffer.offset())
    }

    /// Returns working details cols
    pub fn get_cols(&self) -> u16 {
        self.working_details.cols
    }

    /// Returns working details col width
    pub fn get_width(&self) -> usize {
        self.working_details.col_width
    }

    /// Calculates how many rows the Menu will use
    pub fn get_rows(&self, line_buffer: &LineBuffer) -> u16 {
        let rows = self.get_values(line_buffer).len() as f64 / self.working_details.cols as f64;
        rows.ceil() as u16
    }

    /// Minimum rows that should be displayed by the menu
    pub fn min_rows(&self, line_buffer: &LineBuffer) -> u16 {
        self.get_rows(line_buffer).min(self.min_rows)
    }

    /// Reset menu position
    pub fn reset_position(&mut self) {
        self.col_pos = 0;
        self.row_pos = 0;
    }

    /// Menu index based on column and row position
    pub fn position(&self) -> usize {
        let position = self.row_pos * self.working_details.cols + self.col_pos;
        position as usize
    }

    /// Move menu cursor up
    pub fn move_up(&mut self, line_buffer: &LineBuffer) {
        self.row_pos = if let Some(row) = self.row_pos.checked_sub(1) {
            row
        } else {
            self.get_rows(line_buffer).saturating_sub(1)
        }
    }

    /// Move menu cursor left
    pub fn move_down(&mut self, line_buffer: &LineBuffer) {
        let new_row = self.row_pos + 1;
        self.row_pos = if new_row >= self.get_rows(line_buffer) {
            0
        } else {
            new_row
        }
    }

    /// Move menu cursor left
    pub fn move_left(&mut self) {
        self.col_pos = if let Some(row) = self.col_pos.checked_sub(1) {
            row
        } else {
            self.working_details.cols.saturating_sub(1)
        }
    }

    /// Move menu cursor to the next element
    pub fn move_next(&mut self, line_buffer: &LineBuffer) {
        let mut new_col = self.col_pos + 1;
        let mut new_row = self.row_pos;

        if self.col_pos + 1 >= self.working_details.cols {
            new_row += 1;
            new_col = 0;
        }

        if new_row >= self.get_rows(line_buffer) {
            new_row = 0;
            new_col = 0;
        }

        self.col_pos = new_col;
        self.row_pos = new_row;
    }

    /// Move menu cursor element
    pub fn move_right(&mut self) {
        let new_col = self.col_pos + 1;
        self.col_pos = if new_col >= self.working_details.cols {
            0
        } else {
            new_col
        }
    }

    /// Get selected value from filler
    pub fn get_value(&self, line_buffer: &LineBuffer) -> Option<(Span, String)> {
        self.get_values(line_buffer).get(self.position()).cloned()
    }

    /// Text style for menu
    pub fn text_style(&self, index: usize) -> &str {
        if index == self.position() {
            &self.color.selection_style
        } else {
            &self.color.text_style
        }
    }

    /// End of line for menu
    pub fn end_of_line(&self, column: u16) -> &str {
        if column == self.working_details.cols.saturating_sub(1) {
            "\r\n"
        } else {
            ""
        }
    }

    /// Printable width for a line
    pub fn printable_width(&self, line: &str) -> usize {
        let printable_width = self.working_details.col_width - self.working_details.col_padding;
        printable_width.min(line.len())
    }
}
