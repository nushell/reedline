use nu_ansi_term::Color;

/// Struct to store coloring for the menu
struct MenuTextColor {
    selection_style: String,
    text_style: String,
}

impl Default for MenuTextColor {
    fn default() -> Self {
        Self {
            selection_style: Color::Red.bold().prefix().to_string(),
            text_style: Color::DarkGray.prefix().to_string(),
        }
    }
}

/// Context menu definition
pub struct ContextMenu {
    filler: Box<dyn MenuFiller>,
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
    /// Number of columns that the menu will have
    pub cols: u16,
    /// Column width
    pub col_width: usize,
}

impl Default for ContextMenu {
    fn default() -> Self {
        let filler = Box::new(ExampleData::new());
        Self::new_with(filler)
    }
}

impl ContextMenu {
    /// Creates a context menu with a filler
    pub fn new_with(filler: Box<dyn MenuFiller>) -> Self {
        Self {
            filler,
            active: false,
            color: MenuTextColor::default(),
            min_rows: 3,
            col_pos: 0,
            row_pos: 0,
            cols: 4,
            col_width: 15,
        }
    }

    /// Activates context menu
    pub fn activate(&mut self) {
        self.active = true;
        self.reset_position();
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
    pub fn get_values(&self) -> Vec<&str> {
        self.filler.context_values()
    }

    /// Calculates how many rows the Menu will use
    pub fn get_rows(&self) -> u16 {
        let rows = self.get_values().len() as f64 / self.cols as f64;
        rows.ceil() as u16
    }

    /// Minimum rows that should be displayed by the menu
    pub fn min_rows(&self) -> u16 {
        self.get_rows().min(self.min_rows)
    }

    /// Reset menu position
    pub fn reset_position(&mut self) {
        self.col_pos = 0;
        self.row_pos = 0;
    }

    /// Menu index based on column and row position
    pub fn position(&self) -> usize {
        let position = self.row_pos * self.cols + self.col_pos;
        position as usize
    }

    /// Move menu cursor up
    pub fn move_up(&mut self) {
        self.row_pos = if let Some(row) = self.row_pos.checked_sub(1) {
            row
        } else {
            self.get_rows().saturating_sub(1)
        }
    }

    /// Move menu cursor left
    pub fn move_down(&mut self) {
        let new_row = self.row_pos + 1;
        self.row_pos = if new_row >= self.get_rows() {
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
            self.cols.saturating_sub(1)
        }
    }

    /// Move menu cursor right
    pub fn move_right(&mut self) {
        let new_col = self.col_pos + 1;
        self.col_pos = if new_col >= self.cols { 0 } else { new_col }
    }

    /// Get selected value from filler
    pub fn get_value(&self) -> Option<&str> {
        self.get_values().get(self.position()).copied()
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
        if column == self.cols.saturating_sub(1) {
            "\r\n"
        } else {
            ""
        }
    }

    /// Printable width for a line
    pub fn printable_width(&self, line: &str) -> usize {
        let printable_width = (self.col_width - 2) as usize;
        printable_width.min(line.len())
    }
}

/// The MenuFiller is a trait that defines how the data for the context menu
/// will be collected.
pub trait MenuFiller: Send {
    /// Collects menu values
    fn context_values(&self) -> Vec<&str>;
}

/// Data example for Reedline ContextMenu
struct ExampleData {}

impl MenuFiller for ExampleData {
    fn context_values(&self) -> Vec<&str> {
        vec![
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        ]
    }
}

impl ExampleData {
    /// Creates new instance of Example Menu
    pub fn new() -> Self {
        ExampleData {}
    }
}
