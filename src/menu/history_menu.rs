use super::{Menu, MenuTextStyle};
use crate::{
    painter::{estimate_single_line_wraps, Painter},
    Completer, History, LineBuffer, Span,
};
use nu_ansi_term::{ansi::RESET, Style};
use std::iter::Sum;

enum MenuEvent {
    Edit,
    NextElement,
    PreviousElement,
    NextPage,
    PreviousPage,
}

struct Page {
    size: usize,
    full: bool,
}

impl<'a> Sum<&'a Page> for Page {
    fn sum<I>(iter: I) -> Page
    where
        I: Iterator<Item = &'a Page>,
    {
        iter.fold(
            Page {
                size: 0,
                full: false,
            },
            |acc, menu| Page {
                size: acc.size + menu.size,
                full: acc.full || menu.full,
            },
        )
    }
}

/// Struct to store the menu style

/// Context menu definition
pub struct HistoryMenu {
    /// Menu coloring
    color: MenuTextStyle,
    /// Number of history records pulled until page is full
    page_size: usize,
    /// Menu marker displayed when the menu is active
    marker: String,
    /// Character that will start a selection via a number. E.g let:5 will select
    /// the fifth entry in the current page
    row_char: char,
    /// History menu active status
    active: bool,
    /// Cached values collected when querying the history.
    /// When collecting chronological values, the menu only caches at least
    /// page_size records.
    /// When performing a query to the history object, the cached values will
    /// be the result from such query
    values: Vec<(Span, String)>,
    /// row position in the menu. Starts from 0
    row_position: u16,
    /// Max size of the history when querying without a search buffer
    history_size: Option<usize>,
    /// Max number of lines that are shown with large history entries
    max_lines: u16,
    /// Multiline marker
    multiline_marker: String,
    /// Registry of the number of entries per page that have been displayed
    pages: Vec<Page>,
    /// Page index
    page: usize,
    /// Event sent to the menu
    event: Option<MenuEvent>,
}

impl Default for HistoryMenu {
    fn default() -> Self {
        Self {
            color: MenuTextStyle::default(),
            page_size: 10,
            row_char: ':',
            active: false,
            values: Vec::new(),
            row_position: 0,
            page: 0,
            history_size: None,
            marker: "? ".to_string(),
            max_lines: 5,
            multiline_marker: ":::".to_string(),
            pages: Vec::new(),
            event: None,
        }
    }
}

impl HistoryMenu {
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

    /// Menu builder with page size
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Menu builder with row char
    pub fn with_row_char(mut self, row_char: char) -> Self {
        self.row_char = row_char;
        self
    }

    /// Menu builder with menu marker
    pub fn with_marker(mut self, marker: String) -> Self {
        self.marker = marker;
        self
    }

    /// Menu builder with max entry lines
    pub fn with_max_entry_lines(mut self, max_lines: u16) -> Self {
        self.max_lines = max_lines;
        self
    }

    fn update_row_pos(&mut self, new_pos: Option<usize>) {
        if let (Some(row), Some(page)) = (new_pos, self.pages.get(self.page)) {
            if row < page.size {
                self.row_position = row as u16
            }
        }
    }

    fn create_values_no_query(&mut self, history: &dyn History) -> Vec<String> {
        // When there is no line buffer it is better to get a partial list of all
        // the values that can be queried from the history. There is no point to
        // replicate the whole entries list in the history menu
        let skip = self.pages.iter().take(self.page).sum::<Page>().size;
        let take = self
            .pages
            .get(self.page)
            .map(|page| page.size)
            .unwrap_or(self.page_size);

        history
            .iter_chronologic()
            .rev()
            .skip(skip)
            .take(take)
            .cloned()
            .collect::<Vec<String>>()
    }

    /// The number of rows an entry from the menu can take considering wrapping
    fn number_of_lines(&self, entry: &str, terminal_columns: u16) -> u16 {
        number_of_lines(entry, self.max_lines as usize, terminal_columns)
    }

    fn total_values(&self) -> usize {
        self.history_size.unwrap_or(self.values.len())
    }

    fn values_until_current_page(&self) -> usize {
        self.pages.iter().take(self.page + 1).sum::<Page>().size
    }

    fn set_actual_page_size(&mut self, printable_entries: usize) {
        if let Some(page) = self.pages.get_mut(self.page) {
            page.full = page.size > printable_entries;
            page.size = printable_entries;
        }
    }

    /// Reset menu position
    fn reset_position(&mut self) {
        self.page = 0;
        self.row_position = 0;
        self.pages = Vec::new();
        self.event = None;
    }

    fn printable_entries(&self, painter: &Painter) -> usize {
        let available_lines = painter.screen_height().saturating_sub(1);
        let (printable_entries, _) =
            self.get_values()
                .iter()
                .fold(
                    (0, Some(0)),
                    |(lines, total_lines), (_, entry)| match total_lines {
                        None => (lines, None),
                        Some(total_lines) => {
                            let new_total_lines =
                                total_lines + self.number_of_lines(entry, painter.screen_width());

                            if new_total_lines < available_lines {
                                (lines + 1, Some(new_total_lines))
                            } else {
                                (lines, None)
                            }
                        }
                    },
                );

        printable_entries
    }

    fn no_page_msg(&self, use_ansi_coloring: bool) -> String {
        let msg = "PAGE NOT FOUND";
        if use_ansi_coloring {
            format!(
                "{}{}{}",
                self.color.selected_text_style.prefix(),
                msg,
                RESET
            )
        } else {
            msg.to_string()
        }
    }

    fn banner_message(&self, page: &Page, use_ansi_coloring: bool) -> String {
        let values_until = self.values_until_current_page();
        let value_before = if self.values.is_empty() {
            0
        } else {
            let page_size = self.pages.get(self.page).map(|page| page.size).unwrap_or(0);
            values_until.saturating_sub(page_size) + 1
        };

        let full_page = if page.full { "[FULL]" } else { "" };
        let status_bar = format!(
            "records {} - {}  total: {}  {}",
            value_before,
            values_until,
            self.total_values(),
            full_page,
        );

        if use_ansi_coloring {
            format!(
                "{}{}{}",
                self.color.selected_text_style.prefix(),
                status_bar,
                RESET,
            )
        } else {
            status_bar
        }
    }

    /// Creates default string that represents one line from a menu
    fn create_string(
        &self,
        line: &str,
        index: usize,
        row_number: &str,
        column: u16,
        empty_space: usize,
        use_ansi_coloring: bool,
    ) -> String {
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
    }
}

impl Menu for HistoryMenu {
    fn name(&self) -> &str {
        "history_menu"
    }

    /// Menu indicator
    fn indicator(&self) -> &str {
        self.marker.as_str()
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

    /// Collecting the value from the history to be shown in the menu
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        _completer: &dyn Completer,
    ) {
        let (query, row) = parse_row_selector(line_buffer.get_buffer(), &self.row_char);
        self.update_row_pos(row);
        if let Some(MenuEvent::Edit) = self.event {
            if row.is_none() {
                self.reset_position()
            }
        }

        let values = if query.is_empty() {
            self.history_size = Some(history.max_values());
            self.create_values_no_query(history)
        } else {
            self.history_size = None;
            history.query_entries(query)
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

    /// Gets values from cached values that will be displayed in the menu
    fn get_values(&self) -> &[(Span, String)] {
        if self.history_size.is_some() {
            // When there is a history size value it means that only a chunk of the
            // chronological data from the database was collected
            &self.values
        } else {
            // If no history record then it means that the values hold the result
            // from the query to the database. This slice can be used to get the
            // data that will be shown in the menu
            if self.values.is_empty() {
                return &self.values;
            }

            let start = self.pages.iter().take(self.page).sum::<Page>().size;

            let end: usize = if self.page >= self.pages.len() {
                self.page_size + start
            } else {
                self.pages.iter().take(self.page + 1).sum::<Page>().size
            };

            let end = end.min(self.total_values());
            &self.values[start..end]
        }
    }

    /// Move menu cursor up
    fn edit_line_buffer(&mut self) {
        self.event = Some(MenuEvent::Edit);
    }

    /// Move menu cursor up
    fn move_up(&mut self) {
        self.move_previous()
    }

    /// Move menu cursor down
    fn move_down(&mut self) {
        self.move_next()
    }

    /// Move menu cursor left
    fn move_left(&mut self) {
        self.move_previous()
    }

    /// Move menu cursor right
    fn move_right(&mut self) {
        self.move_next()
    }

    /// Move menu cursor to the next element
    fn move_next(&mut self) {
        self.event = Some(MenuEvent::NextElement);
    }

    /// Move menu cursor to the previous element
    fn move_previous(&mut self) {
        self.event = Some(MenuEvent::PreviousElement);
    }

    /// Moves to the next history page
    fn next_page(&mut self) {
        self.event = Some(MenuEvent::NextPage);
    }

    /// Moves to the previous history page
    fn previous_page(&mut self) {
        self.event = Some(MenuEvent::PreviousPage);
    }

    /// The buffer gets cleared with the actual value
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer) {
        if let Some((_, value)) = self.get_value() {
            line_buffer.set_buffer(value)
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

    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    ) {
        match &self.event {
            Some(event) => match event {
                MenuEvent::NextElement => {
                    let new_pos = self.row_position + 1;

                    if let Some(page) = self.pages.get(self.page) {
                        if new_pos >= page.size as u16 {
                            self.row_position = 0
                        } else {
                            self.row_position = new_pos
                        }
                    }
                }
                MenuEvent::PreviousElement => {
                    if let Some(page) = self.pages.get(self.page) {
                        if let Some(new_pos) = self.row_position.checked_sub(1) {
                            self.row_position = new_pos
                        } else {
                            self.row_position = page.size.saturating_sub(1) as u16
                        }
                    }
                }
                MenuEvent::NextPage => {
                    if self.values_until_current_page() <= self.total_values().saturating_sub(1) {
                        match self.pages.get_mut(self.page) {
                            Some(page) => {
                                if !page.full {
                                    page.size += self.page_size;
                                } else {
                                    self.page += 1;
                                    self.pages.push(Page {
                                        size: self.page_size,
                                        full: false,
                                    })
                                }
                            }
                            None => self.pages.push(Page {
                                size: self.page_size,
                                full: false,
                            }),
                        }

                        self.update_values(line_buffer, history, completer);
                        self.set_actual_page_size(self.printable_entries(painter));
                    } else {
                        self.page = 0;
                        self.update_values(line_buffer, history, completer);
                    }
                }
                MenuEvent::PreviousPage => {
                    self.page = self.page.saturating_sub(1);
                    self.update_values(line_buffer, history, completer);
                }
                MenuEvent::Edit => {
                    self.update_values(line_buffer, history, completer);
                    self.pages.push(Page {
                        size: self.printable_entries(painter),
                        full: false,
                    });
                }
            },
            None => {
                self.pages.push(Page {
                    size: self.printable_entries(painter),
                    full: false,
                });
            }
        }

        self.event = None
    }

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal and if an entry is larger than the remaining lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.get_values().iter().fold(0, |acc, (_, entry)| {
            acc + self.number_of_lines(entry, terminal_columns)
        }) + 1
    }

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, _available_lines: u16, use_ansi_coloring: bool) -> String {
        match self.pages.get(self.page) {
            Some(page) => {
                let lines_string = self
                    .get_values()
                    .iter()
                    .take(page.size)
                    .enumerate()
                    .map(|(index, (_, line))| {
                        let empty_space = self.get_width().saturating_sub(line.len());

                        // Final string with colors
                        let line = if line.lines().count() > self.max_lines as usize {
                            let lines = line
                                .lines()
                                .take(self.max_lines as usize)
                                .map(|string| format!("{}\r\n{}", string, self.multiline_marker))
                                .collect::<String>();

                            lines + "..."
                        } else {
                            line.replace("\n", &format!("\r\n{}", self.multiline_marker))
                        };

                        let row_number = format!("{}: ", index);

                        self.create_string(
                            &line,
                            index,
                            &row_number,
                            0,
                            empty_space,
                            use_ansi_coloring,
                        )
                    })
                    .collect::<String>();

                format!(
                    "{}{}",
                    lines_string,
                    self.banner_message(page, use_ansi_coloring)
                )
            }
            None => self.no_page_msg(use_ansi_coloring),
        }
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.max_lines + 1
    }

    /// Row position
    fn row_pos(&self) -> u16 {
        self.row_position
    }

    /// Column position
    fn get_cols(&self) -> u16 {
        1
    }

    /// Column position
    fn col_pos(&self) -> u16 {
        0
    }

    /// Returns working details col width
    fn get_width(&self) -> usize {
        50
    }
}

fn parse_row_selector<'buffer>(
    buffer: &'buffer str,
    marker: &char,
) -> (&'buffer str, Option<usize>) {
    if buffer.is_empty() {
        return (buffer, None);
    }

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
                    return (&buffer[0..index], Some(0));
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

fn number_of_lines(entry: &str, max_lines: usize, terminal_columns: u16) -> u16 {
    let lines = if entry.contains('\n') {
        let total_lines = entry.lines().count();
        let printable_lines = if total_lines > max_lines {
            // The extra one is there because when printing a large entry and extra line
            // is added with ...
            max_lines + 1
        } else {
            total_lines
        };

        let wrap_lines = entry.lines().take(max_lines).fold(0, |acc, line| {
            acc + estimate_single_line_wraps(line, terminal_columns)
        });

        (printable_lines + wrap_lines) as u16
    } else {
        1 + estimate_single_line_wraps(entry, terminal_columns) as u16
    };

    lines
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
        assert_eq!(row, Some(0))
    }

    #[test]
    fn parse_empty_buffer_test() {
        let input = "";
        let (res, row) = parse_row_selector(input, &':');

        assert_eq!(res, "");
        assert_eq!(row, None)
    }

    #[test]
    fn number_of_lines_test() {
        let input = "let a: another:\nsomething\nanother";
        let res = number_of_lines(input, 5, 30);

        // There is an extra line showing ...
        assert_eq!(res, 3);
    }

    #[test]
    fn number_one_line_test() {
        let input = "let a: another";
        let res = number_of_lines(input, 5, 30);

        assert_eq!(res, 1);
    }

    #[test]
    fn lines_with_wrap_test() {
        let input = "let a= an1other ver2y large l3ine what 4should wr5ap";
        let res = number_of_lines(input, 5, 10);

        assert_eq!(res, 6);
    }

    #[test]
    fn number_of_max_lines_test() {
        let input = "let a\n: ano\nther:\nsomething\nanother\nmore\nanother\nasdf\nasdfa\n3123";
        let res = number_of_lines(input, 3, 30);

        // There is an extra line showing ...
        assert_eq!(res, 4);
    }
}
