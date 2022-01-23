use super::{Menu, MenuTextStyle};
use crate::{History, LineBuffer, Span};

/// Struct to store the menu style

/// Context menu definition
pub struct HistoryMenu {
    active: bool,
    /// Context menu coloring
    color: MenuTextStyle,
    /// Menu cached values collected when querying the history
    values: Vec<(Span, String)>,
    /// row position in the menu. Starts from 0
    row_pos: u16,
    /// The collected values from the history are split in pages
    page: usize,
    /// Number of history records presented per page
    page_size: usize,
}

impl Default for HistoryMenu {
    fn default() -> Self {
        Self {
            active: false,
            color: MenuTextStyle::default(),
            values: Vec::new(),
            row_pos: 0,
            page: 0,
            page_size: 3,
        }
    }
}

impl HistoryMenu {
    /// Creates a context menu with a filler
    pub fn new_with() -> Self {
        Self {
            active: false,
            color: MenuTextStyle::default(),
            values: Vec::new(),
            row_pos: 0,
            page: 0,
            page_size: 3,
        }
    }

    /// Collecting the value from the history to be shown in the menu
    pub fn update_values(&mut self, history: &dyn History, line_buffer: &LineBuffer) {
        let values = if line_buffer.is_empty() {
            history
                .iter_chronologic()
                .rev()
                .cloned()
                .collect::<Vec<String>>()
        } else {
            history.query_entries(line_buffer.get_buffer())
        };

        self.values = values
            .into_iter()
            .map(|s| {
                (
                    Span {
                        start: 0,
                        end: s.len(),
                    },
                    s,
                )
            })
            .collect();

        self.reset_position();
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
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Move menu cursor to the next element
    pub fn move_next(&mut self) {
        let new_pos = self.row_pos + 1;

        if new_pos >= self.get_num_values() as u16 {
            self.row_pos = 0
        } else {
            self.row_pos = new_pos
        }
    }

    /// Move menu cursor to the previous element
    pub fn move_previous(&mut self) {
        if let Some(new_pos) = self.row_pos.checked_sub(1) {
            self.row_pos = new_pos
        } else {
            self.row_pos = self.get_num_values().saturating_sub(1) as u16
        }
    }

    /// Moves to the next history page
    pub fn next_page(&mut self) {
        let pages = (self.values.len() / self.page_size) + 1;
        if self.page + 1 < pages {
            self.page += 1;
        }
    }

    /// Moves to the previous history page
    pub fn previous_page(&mut self) {
        self.page = self.page.saturating_sub(1)
    }

    /// Reset menu position
    fn reset_position(&mut self) {
        self.row_pos = 0;
        self.page = 0;
    }
}

impl Menu for HistoryMenu {
    /// Text style for menu
    fn text_style(&self, index: usize) -> String {
        if index == self.position() {
            self.color.selected_text_style.prefix().to_string()
        } else {
            self.color.text_style.prefix().to_string()
        }
    }

    // The rows for the history menu may be multiline an require to consider wrapping
    fn get_rows(&self) -> u16 {
        let rows = self
            .get_values()
            .iter()
            .skip(self.page * self.page_size)
            .take(self.page_size)
            .fold(0, |acc, (_, string)| acc + string.lines().count());

        rows as u16 + 1
    }

    fn print_enumerate(&self) -> bool {
        true
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.get_rows().min(self.page_size as u16)
    }

    /// Row position
    fn row_pos(&self) -> u16 {
        self.row_pos
    }

    /// Column position
    fn col_pos(&self) -> u16 {
        0
    }

    /// Gets values from filler that will be displayed in the menu
    fn get_values(&self) -> &[(Span, String)] {
        let start = self.page * self.page_size;

        // The end of the slice of values is limited by the total number of
        // values in the queried values
        let end = start + self.page_size;
        let end = end.min(self.values.len());

        if end >= start {
            &self.values[start..end]
        } else {
            &self.values[end.saturating_sub(self.page_size)..end]
        }
    }

    /// Returns working details columns
    fn get_cols(&self) -> u16 {
        1
    }

    /// Returns working details col width
    fn get_width(&self) -> usize {
        50
    }
}
