use nu_ansi_term::Style;

use super::{Menu, MenuTextStyle};
use crate::{History, LineBuffer, Span};

/// Struct to store the menu style

/// Context menu definition
pub struct HistoryMenu {
    /// Context menu coloring
    color: MenuTextStyle,
    /// Number of history records presented per page
    page_size: usize,
    /// Select row character
    row_char: char,
    /// History menu active status
    active: bool,
    /// Menu cached values collected when querying the history
    values: Vec<(Span, String)>,
    /// row position in the menu. Starts from 0
    row_pos: u16,
    /// The collected values from the history are split in pages
    page: usize,
    /// Max size of the history when querying without a search buffer
    history_size: Option<usize>,
}

impl Default for HistoryMenu {
    fn default() -> Self {
        Self {
            color: MenuTextStyle::default(),
            page_size: 10,
            row_char: ':',
            active: false,
            values: Vec::new(),
            row_pos: 0,
            page: 0,
            history_size: None,
        }
    }
}

/// Struct used to set default values for the history menu.
/// The default values, such as style or column details are used to calculate
/// the working values for the menu
#[derive(Default)]
pub struct HistoryMenuInput {
    menu_style: MenuTextStyle,
    page_size: usize,
    row_char: char,
}

impl HistoryMenuInput {
    /// Context Menu builder with new value for text style
    pub fn with_text_style(mut self, text_style: Style) -> Self {
        self.menu_style.text_style = text_style;
        self
    }

    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    pub fn with_row_char(mut self, row_char: char) -> Self {
        self.row_char = row_char;
        self
    }
}

impl HistoryMenu {
    /// Creates a context menu with a filler
    pub fn new_with(input: HistoryMenuInput) -> Self {
        Self {
            color: input.menu_style,
            page_size: input.page_size,
            row_char: input.row_char,
            ..Default::default()
        }
    }

    /// Collecting the value from the history to be shown in the menu
    pub fn update_values(&mut self, history: &dyn History, line_buffer: &LineBuffer) {
        let values = if line_buffer.is_empty() {
            self.reset_position();
            self.create_values_no_query(history)
        } else {
            let (query, row) = self.parse_row_selector(line_buffer.get_buffer());

            self.update_row_pos(row);
            if query.is_empty() {
                self.create_values_no_query(history)
            } else {
                self.page = 0;
                self.history_size = None;
                history.query_entries(query)
            }
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
    }

    fn update_row_pos(&mut self, new_pos: Option<usize>) {
        if let Some(row) = new_pos {
            if row < self.page_size {
                self.row_pos = row as u16
            }
        }
    }

    fn parse_row_selector<'buffer>(&self, buffer: &'buffer str) -> (&'buffer str, Option<usize>) {
        let mut input = buffer.chars().peekable();

        let mut index = 0;
        while let Some(char) = input.next() {
            if char == self.row_char {
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

    fn create_values_no_query(&mut self, history: &dyn History) -> Vec<String> {
        // When there is no line buffer it is better to get a partial list of all
        // the values that can be queried from the history. There is no point to
        // replicate the whole entries list in the history menu
        self.history_size = Some(history.max_values());
        history
            .iter_chronologic()
            .rev()
            .skip(self.page * self.page_size)
            .take(self.page_size)
            .cloned()
            .collect::<Vec<String>>()
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
        let values = match self.history_size {
            Some(size) => size,
            None => self.values.len(),
        };

        let pages = (values / self.page_size) + 1;
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
        if self.history_size.is_some() {
            &self.values
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_row_test() {
        let history_menu = HistoryMenu::default();
        let input = "search:6";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "search");
        assert_eq!(row, Some(6))
    }

    #[test]
    fn parse_row_other_marker_test() {
        let menu_input = HistoryMenuInput::default().with_row_char('?');
        let history_menu = HistoryMenu::new_with(menu_input);
        let input = "search?9";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "search");
        assert_eq!(row, Some(9))
    }

    #[test]
    fn parse_row_double_test() {
        let history_menu = HistoryMenu::default();
        let input = "ls | where:16";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "ls | where");
        assert_eq!(row, Some(16))
    }

    #[test]
    fn parse_row_empty_test() {
        let history_menu = HistoryMenu::default();
        let input = ":10";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "");
        assert_eq!(row, Some(10))
    }

    #[test]
    fn parse_row_fake_indicator_test() {
        let history_menu = HistoryMenu::default();
        let input = "let a: another :10";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "let a: another ");
        assert_eq!(row, Some(10))
    }

    #[test]
    fn parse_row_no_number_test() {
        let history_menu = HistoryMenu::default();
        let input = "let a: another:";
        let (res, row) = history_menu.parse_row_selector(input);

        assert_eq!(res, "let a: another");
        assert_eq!(row, None)
    }
}
