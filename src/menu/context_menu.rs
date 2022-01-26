use nu_ansi_term::Style;

use super::{Menu, MenuTextStyle};
use crate::{Completer, DefaultCompleter, History, LineBuffer, Span};

/// Default values used as reference for the menu. These values are set during
/// the initial declaration of the menu and are always kept as reference for the
/// changeable ColumnDetail
struct DefaultColumnDetails {
    /// Number of columns that the menu will have
    pub columns: u16,
    /// Column width
    pub col_width: Option<usize>,
    /// Column padding
    pub col_padding: usize,
}

impl Default for DefaultColumnDetails {
    fn default() -> Self {
        Self {
            columns: 4,
            col_width: None,
            col_padding: 2,
        }
    }
}

/// Represents the actual column conditions of the menu. These conditions change
/// since they need to accommodate possible different line sizes for the column values
#[derive(Default)]
struct ColumnDetails {
    /// Number of columns that the menu will have
    pub columns: u16,
    /// Column width
    pub col_width: usize,
    /// Column padding
    pub col_padding: usize,
}

/// Context menu definition
pub struct ContextMenu {
    completer: Box<dyn Completer>,
    active: bool,
    /// Context menu coloring
    color: MenuTextStyle,
    /// Default column details that are set when creating the menu
    /// These values are the reference for the working details
    default_details: DefaultColumnDetails,
    /// Number of minimum rows that are displayed when
    /// the required lines is larger than the available lines
    min_rows: u16,
    /// Working column details keep changing based on the collected values
    working_details: ColumnDetails,
    /// Menu cached values
    values: Vec<(Span, String)>,
    /// column position of the cursor. Starts from 0
    col_pos: u16,
    /// row position in the menu. Starts from 0
    row_pos: u16,
}

impl Default for ContextMenu {
    fn default() -> Self {
        let completer = Box::new(DefaultCompleter::default());

        Self {
            completer,
            active: false,
            color: MenuTextStyle::default(),
            default_details: DefaultColumnDetails::default(),
            min_rows: 3,
            working_details: ColumnDetails::default(),
            values: Vec::new(),
            col_pos: 0,
            row_pos: 0,
        }
    }
}

impl ContextMenu {
    /// Builder to assign Completer
    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> Self {
        self.completer = completer;
        self
    }

    /// Menu builder with new value for text style
    pub fn with_text_style(mut self, text_style: Style) -> Self {
        self.color.text_style = text_style;
        self
    }

    /// Menu builder with new value for text style
    pub fn with_selected_text_style(mut self, selected_text_style: Style) -> Self {
        self.color.selected_text_style = selected_text_style;
        self
    }

    /// Menu builder with new columns value
    pub fn with_columns(mut self, columns: u16) -> Self {
        self.default_details.columns = columns;
        self
    }

    /// Menu builder with new column width value
    pub fn with_column_width(mut self, col_width: Option<usize>) -> Self {
        self.default_details.col_width = col_width;
        self
    }

    /// Menu builder with new column width value
    pub fn with_column_padding(mut self, col_padding: usize) -> Self {
        self.default_details.col_padding = col_padding;
        self
    }

    /// Reset menu position
    fn reset_position(&mut self) {
        self.col_pos = 0;
        self.row_pos = 0;
    }
}

impl Menu for ContextMenu {
    /// Menu name
    fn name(&self) -> &str {
        "context_menu"
    }

    /// Menu indicator
    fn indicator(&self) -> &str {
        "| "
    }

    /// Deactivates context menu
    fn is_active(&self) -> bool {
        self.active
    }

    /// Activates context menu
    fn activate(&mut self) {
        self.active = true;
        self.reset_position();
    }

    /// Deactivates context menu
    fn deactivate(&mut self) {
        self.active = false
    }

    /// Updates menu values
    fn update_values(&mut self, line_buffer: &mut LineBuffer, _history: &dyn History) {
        // If there is a new line character in the line buffer, the completer
        // doesn't calculate the suggested values correctly. This happens when
        // editing a multiline buffer.
        // Also, by replacing the new line character with a space, the insert
        // position is maintain in the line buffer.
        let trimmed_buffer = line_buffer.get_buffer().replace("\n", " ");
        self.values = self
            .completer
            .complete(trimmed_buffer.as_str(), line_buffer.offset());
        self.reset_position();
    }

    /// The working details for the menu changes based on the size of the lines
    /// collected from the completer
    fn update_working_details(&mut self, screen_width: u16) {
        let max_width = self.get_values().iter().fold(0, |acc, (_, string)| {
            let str_len = string.len() + self.working_details.col_padding;
            if str_len > acc {
                str_len
            } else {
                acc
            }
        });

        // If no default width if found, then the total screen width is used to estimate
        // the column width based on the default number of columns
        let default_width = match self.default_details.col_width {
            Some(col_width) => col_width,
            None => {
                let col_width = screen_width / self.default_details.columns;
                col_width as usize
            }
        };

        // Adjusting the working width of the column based the max line width found
        // in the menu values
        if max_width > default_width {
            self.working_details.col_width = max_width;
        } else {
            self.working_details.col_width = default_width;
        };

        // The working columns is adjusted based on possible number of columns
        // that could be fitted in the screen with the calculated column width
        let possible_cols = screen_width / self.working_details.col_width as u16;
        if possible_cols > self.default_details.columns {
            self.working_details.columns = self.default_details.columns.max(1);
        } else {
            self.working_details.columns = possible_cols;
        }
    }

    /// Move menu cursor to the next element
    fn move_next(&mut self) {
        let mut new_col = self.col_pos + 1;
        let mut new_row = self.row_pos;

        if new_col >= self.get_cols() {
            new_row += 1;
            new_col = 0;
        }

        if new_row >= self.get_rows() {
            new_row = 0;
            new_col = 0;
        }

        let position = new_row * self.get_cols() + new_col;
        if position >= self.get_values().len() as u16 {
            self.reset_position();
        } else {
            self.col_pos = new_col;
            self.row_pos = new_row;
        }
    }

    /// Move menu cursor to the previous element
    fn move_previous(&mut self) {
        let new_col = self.col_pos.checked_sub(1);

        let (new_col, new_row) = match new_col {
            Some(col) => (col, self.row_pos),
            None => match self.row_pos.checked_sub(1) {
                Some(row) => (self.get_cols().saturating_sub(1), row),
                None => (
                    self.get_cols().saturating_sub(1),
                    self.get_rows().saturating_sub(1),
                ),
            },
        };

        let position = new_row * self.get_cols() + new_col;
        if position >= self.get_values().len() as u16 {
            self.col_pos = (self.get_values().len() as u16 % self.get_cols()).saturating_sub(1);
            self.row_pos = self.get_rows().saturating_sub(1);
        } else {
            self.col_pos = new_col;
            self.row_pos = new_row;
        }
    }

    /// The buffer gets replaced in the Span location
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer) {
        if let Some((span, value)) = self.get_value() {
            let mut offset = line_buffer.offset();
            offset += value.len() - (span.end - span.start);

            line_buffer.replace(span.start..span.end, &value);
            line_buffer.set_insertion_point(offset);
        }
    }

    /// Move menu cursor up
    fn move_up(&mut self) {
        self.row_pos = if let Some(row) = self.row_pos.checked_sub(1) {
            row
        } else {
            self.get_rows().saturating_sub(1)
        }
    }

    /// Move menu cursor left
    fn move_down(&mut self) {
        let new_row = self.row_pos + 1;
        self.row_pos = if new_row >= self.get_rows() {
            0
        } else {
            new_row
        }
    }

    /// Move menu cursor left
    fn move_left(&mut self) {
        self.col_pos = if let Some(row) = self.col_pos.checked_sub(1) {
            row
        } else {
            self.get_cols().saturating_sub(1)
        }
    }

    /// Move menu cursor element
    fn move_right(&mut self) {
        let new_col = self.col_pos + 1;
        self.col_pos = if new_col >= self.get_cols() {
            0
        } else {
            new_col
        }
    }

    /// Text style for menu
    fn text_style(&self, index: usize) -> String {
        if index == self.position() {
            self.color.selected_text_style.prefix().to_string()
        } else {
            self.color.text_style.prefix().to_string()
        }
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.get_rows().min(self.min_rows)
    }

    /// Row position
    fn row_pos(&self) -> u16 {
        self.row_pos
    }

    /// Column position
    fn col_pos(&self) -> u16 {
        self.col_pos
    }

    /// Gets values from filler that will be displayed in the menu
    fn get_values(&self) -> &[(Span, String)] {
        &self.values
    }

    /// Returns working details columns
    fn get_cols(&self) -> u16 {
        self.working_details.columns.max(1)
    }

    /// Returns working details col width
    fn get_width(&self) -> usize {
        self.working_details.col_width
    }
}
