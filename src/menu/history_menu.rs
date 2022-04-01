use super::{
    menu_functions::{parse_selection_char, string_difference},
    Menu, MenuEvent, MenuTextStyle,
};
use crate::{
    painting::{estimate_single_line_wraps, Painter},
    Completer, History, LineBuffer, Span, Suggestion,
};
use nu_ansi_term::{ansi::RESET, Style};
use std::iter::Sum;

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
    /// Character that will start a selection via a number. E.g let!5 will select
    /// the fifth entry in the current page
    selection_char: char,
    /// History menu active status
    active: bool,
    /// Cached values collected when querying the history.
    /// When collecting chronological values, the menu only caches at least
    /// page_size records.
    /// When performing a query to the history object, the cached values will
    /// be the result from such query
    values: Vec<Suggestion>,
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
    /// String collected after the menu is activated
    input: Option<String>,
}

impl Default for HistoryMenu {
    fn default() -> Self {
        Self {
            color: MenuTextStyle::default(),
            page_size: 10,
            selection_char: '!',
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
            input: None,
        }
    }
}

impl HistoryMenu {
    /// Menu builder with new value for text style
    #[must_use]
    pub fn with_text_style(mut self, text_style: Style) -> Self {
        self.color.text_style = text_style;
        self
    }

    /// Menu builder with new value for text style
    #[must_use]
    pub fn with_selected_text_style(mut self, selected_text_style: Style) -> Self {
        self.color.selected_text_style = selected_text_style;
        self
    }

    /// Menu builder with page size
    #[must_use]
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Menu builder with row char
    #[must_use]
    pub fn with_selection_char(mut self, selection_char: char) -> Self {
        self.selection_char = selection_char;
        self
    }

    /// Menu builder with menu marker
    #[must_use]
    pub fn with_marker(mut self, marker: String) -> Self {
        self.marker = marker;
        self
    }

    /// Menu builder with max entry lines
    #[must_use]
    pub fn with_max_entry_lines(mut self, max_lines: u16) -> Self {
        self.max_lines = max_lines;
        self
    }

    fn update_row_pos(&mut self, new_pos: Option<usize>) {
        if let (Some(row), Some(page)) = (new_pos, self.pages.get(self.page)) {
            let values_before_page = self.pages.iter().take(self.page).sum::<Page>().size;
            let row = row.saturating_sub(values_before_page);
            if row < page.size {
                self.row_position = row as u16;
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
            page.full = page.size > printable_entries || page.full;
            page.size = printable_entries;
        }
    }

    /// Menu index based on column and row position
    fn index(&self) -> usize {
        self.row_position as usize
    }

    /// Get selected value from the menu
    fn get_value(&self) -> Option<Suggestion> {
        self.get_values().get(self.index()).cloned()
    }

    /// Reset menu position
    fn reset_position(&mut self) {
        self.page = 0;
        self.row_position = 0;
        self.pages = Vec::new();
    }

    fn printable_entries(&self, painter: &Painter) -> usize {
        // The number 2 comes from the prompt line and the banner printed at the bottom
        // of the history menu
        let available_lines = painter.screen_height().saturating_sub(2);
        let (printable_entries, _) =
            self.get_values()
                .iter()
                .fold(
                    (0, Some(0)),
                    |(lines, total_lines), suggestion| match total_lines {
                        None => (lines, None),
                        Some(total_lines) => {
                            let new_total_lines = total_lines
                                + self.number_of_lines(&suggestion.value, painter.screen_width());

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
        let values_until = self.values_until_current_page().saturating_sub(1);
        let value_before = if self.values.is_empty() || self.page == 0 {
            0
        } else {
            let page_size = self.pages.get(self.page).map(|page| page.size).unwrap_or(0);
            values_until.saturating_sub(page_size) + 1
        };

        let full_page = if page.full { "[FULL]" } else { "" };
        let status_bar = format!(
            "Page {}: records {} - {}  total: {}  {}",
            self.page + 1,
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

    /// End of line for menu
    fn end_of_line() -> &'static str {
        "\r\n"
    }

    /// Text style for menu
    fn text_style(&self, index: usize) -> String {
        if index == self.index() {
            self.color.selected_text_style.prefix().to_string()
        } else {
            self.color.text_style.prefix().to_string()
        }
    }

    /// Creates default string that represents one line from a menu
    fn create_string(
        &self,
        line: &str,
        index: usize,
        row_number: &str,
        use_ansi_coloring: bool,
    ) -> String {
        if use_ansi_coloring {
            format!(
                "{}{}{}{}{}{}",
                row_number,
                self.text_style(index),
                &line,
                RESET,
                "",
                Self::end_of_line(),
            )
        } else {
            // If no ansi coloring is found, then the selection word is
            // the line in uppercase
            let line_str = if index == self.index() {
                format!("{}>{}", row_number, line.to_uppercase())
            } else {
                format!("{}{}", row_number, line)
            };

            // Final string with formatting
            format!("{}{}", line_str, Self::end_of_line())
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

    /// There is no use for quick complete for the history menu
    fn can_quick_complete(&self) -> bool {
        false
    }

    /// The history menu should not try to auto complete to avoid comparing
    /// all registered values
    fn can_partially_complete(
        &mut self,
        _values_updated: bool,
        _line_buffer: &mut LineBuffer,
        _history: &dyn History,
        _completer: &dyn Completer,
    ) -> bool {
        false
    }

    /// Selects what type of event happened with the menu
    fn menu_event(&mut self, event: MenuEvent) {
        match &event {
            MenuEvent::Activate(_) => self.active = true,
            MenuEvent::Deactivate => {
                self.active = false;
                self.input = None;
            }
            _ => {}
        }

        self.event = Some(event);
    }

    /// Collecting the value from the history to be shown in the menu
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        _completer: &dyn Completer,
    ) {
        let (start, input) = match &self.input {
            Some(old_string) => string_difference(line_buffer.get_buffer(), old_string),
            None => (line_buffer.insertion_point(), ""),
        };

        let parsed = parse_selection_char(input, self.selection_char);
        self.update_row_pos(parsed.index);

        // If there are no row selector and the menu has an Edit event, this clears
        // the position together with the pages vector
        if matches!(self.event, Some(MenuEvent::Edit(_))) && parsed.index.is_none() {
            self.reset_position();
        }

        let values = if parsed.remainder.is_empty() {
            self.history_size = Some(history.max_values());
            self.create_values_no_query(history)
        } else {
            self.history_size = None;
            history.query_entries(parsed.remainder)
        };

        self.values = values
            .into_iter()
            .map(|value| {
                let span = Span {
                    start,
                    end: start + input.len(),
                };

                Suggestion {
                    value,
                    description: None,
                    span,
                }
            })
            .collect();
    }

    /// Gets values from cached values that will be displayed in the menu
    fn get_values(&self) -> &[Suggestion] {
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

    /// The buffer gets cleared with the actual value
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer) {
        if let Some(Suggestion { value, span, .. }) = self.get_value() {
            line_buffer.replace(span.start..span.end, &value);

            let mut offset = line_buffer.insertion_point();
            offset += value.len() - (span.end - span.start);
            line_buffer.set_insertion_point(offset);
        }
    }

    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    ) {
        if let Some(event) = self.event.clone() {
            match event {
                MenuEvent::Activate(_) => {
                    self.reset_position();
                    self.input = Some(line_buffer.get_buffer().to_string());
                    self.update_values(line_buffer, history, completer);

                    self.pages.push(Page {
                        size: self.printable_entries(painter),
                        full: false,
                    });
                }
                MenuEvent::Deactivate => {
                    self.active = false;
                    self.input = None;
                }
                MenuEvent::Edit(_) => {
                    self.update_values(line_buffer, history, completer);
                    self.pages.push(Page {
                        size: self.printable_entries(painter),
                        full: false,
                    });
                }
                MenuEvent::NextElement | MenuEvent::MoveDown | MenuEvent::MoveRight => {
                    let new_pos = self.row_position + 1;

                    if let Some(page) = self.pages.get(self.page) {
                        if new_pos >= page.size as u16 {
                            self.event = Some(MenuEvent::NextPage);
                            self.update_working_details(line_buffer, history, completer, painter);
                        } else {
                            self.row_position = new_pos;
                        }
                    }
                }
                MenuEvent::PreviousElement | MenuEvent::MoveUp | MenuEvent::MoveLeft => {
                    if let Some(new_pos) = self.row_position.checked_sub(1) {
                        self.row_position = new_pos;
                    } else {
                        let page = if let Some(page) = self.page.checked_sub(1) {
                            self.pages.get(page)
                        } else {
                            self.pages.get(self.pages.len().saturating_sub(1))
                        };

                        if let Some(page) = page {
                            self.row_position = page.size.saturating_sub(1) as u16;
                        }

                        self.event = Some(MenuEvent::PreviousPage);
                        self.update_working_details(line_buffer, history, completer, painter);
                    }
                }
                MenuEvent::NextPage => {
                    if self.values_until_current_page() <= self.total_values().saturating_sub(1) {
                        if let Some(page) = self.pages.get_mut(self.page) {
                            if page.full {
                                self.row_position = 0;
                                self.page += 1;
                                if self.page >= self.pages.len() {
                                    self.pages.push(Page {
                                        size: self.page_size,
                                        full: false,
                                    });
                                }
                            } else {
                                page.size += self.page_size;
                            }
                        }

                        self.update_values(line_buffer, history, completer);
                        self.set_actual_page_size(self.printable_entries(painter));
                    } else {
                        self.row_position = 0;
                        self.page = 0;
                        self.update_values(line_buffer, history, completer);
                    }
                }
                MenuEvent::PreviousPage => {
                    match self.page.checked_sub(1) {
                        Some(page_num) => self.page = page_num,
                        None => self.page = self.pages.len().saturating_sub(1),
                    }
                    self.update_values(line_buffer, history, completer);
                }
            }

            self.event = None;
        }
    }

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal and if an entry is larger than the remaining lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.get_values().iter().fold(0, |acc, suggestion| {
            acc + self.number_of_lines(&suggestion.value, terminal_columns)
        }) + 1
    }

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, _available_lines: u16, use_ansi_coloring: bool) -> String {
        let values_before_page = self.pages.iter().take(self.page).sum::<Page>().size;
        match self.pages.get(self.page) {
            Some(page) => {
                let lines_string = self
                    .get_values()
                    .iter()
                    .take(page.size)
                    .enumerate()
                    .map(|(index, suggestion)| {
                        // Final string with colors
                        let line = &suggestion.value;
                        let line = if line.lines().count() > self.max_lines as usize {
                            let lines = line
                                .lines()
                                .take(self.max_lines as usize)
                                .map(|string| format!("{}\r\n{}", string, self.multiline_marker))
                                .collect::<String>();

                            lines + "..."
                        } else {
                            line.replace('\n', &format!("\r\n{}", self.multiline_marker))
                        };

                        let row_number = format!("{}: ", index + values_before_page);

                        self.create_string(&line, index, &row_number, use_ansi_coloring)
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
