use {
    super::{menu_functions::parse_selection_char, Menu, MenuBuilder, MenuEvent, MenuSettings},
    crate::{
        core_editor::Editor,
        menu_functions::{completer_input, replace_in_buffer},
        painting::{estimate_single_line_wraps, Painter},
        Completer, Suggestion,
    },
    nu_ansi_term::ansi::RESET,
    std::{fmt::Write, iter::Sum},
    unicode_width::UnicodeWidthStr,
};

const SELECTION_CHAR: char = '!';

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
pub struct ListMenu {
    /// Menu settings
    settings: MenuSettings,
    /// Number of records pulled until page is full
    page_size: usize,
    /// Menu active status
    active: bool,
    /// Cached values collected when querying the completer.
    /// When collecting chronological values, the menu only caches at least
    /// page_size records.
    /// When performing a query to the completer, the cached values will
    /// be the result from such query
    values: Vec<Suggestion>,
    /// row position in the menu. Starts from 0
    row_position: u16,
    /// Max size of the suggestions when querying without a search buffer
    query_size: Option<usize>,
    /// Max number of lines that are shown with large suggestions entries
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

impl Default for ListMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default()
                .with_name("search_menu")
                .with_marker("? ")
                .with_only_buffer_difference(true),
            page_size: 10,
            active: false,
            values: Vec::new(),
            row_position: 0,
            page: 0,
            query_size: None,
            max_lines: 5,
            multiline_marker: ":::".to_string(),
            pages: Vec::new(),
            event: None,
            input: None,
        }
    }
}

// Menu configuration functions
impl MenuBuilder for ListMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

// Menu configuration functions
impl ListMenu {
    /// Menu builder with new page size
    #[must_use]
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Menu builder with max entry lines
    #[must_use]
    pub fn with_max_entry_lines(mut self, max_lines: u16) -> Self {
        self.max_lines = max_lines;
        self
    }
}

// Menu functionality
impl ListMenu {
    fn update_row_pos(&mut self, new_pos: Option<usize>) {
        if let (Some(row), Some(page)) = (new_pos, self.pages.get(self.page)) {
            let values_before_page = self.pages.iter().take(self.page).sum::<Page>().size;
            let row = row.saturating_sub(values_before_page);
            if row < page.size {
                self.row_position = row as u16;
            }
        }
    }

    /// The number of rows an entry from the menu can take considering wrapping
    fn number_of_lines(&self, entry: &str, terminal_columns: u16) -> u16 {
        number_of_lines(entry, self.max_lines as usize, terminal_columns)
    }

    fn total_values(&self) -> usize {
        self.query_size.unwrap_or(self.values.len())
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
        // of the menu
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
                                + self.number_of_lines(
                                    &suggestion.value,
                                    //  to account for the index and the indicator e.g. 0: XXXX
                                    painter.screen_width().saturating_sub(
                                        self.indicator().width() as u16 + count_digits(lines),
                                    ),
                                );

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
                self.settings.color.selected_text_style.prefix(),
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
                self.settings.color.selected_text_style.prefix(),
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
            self.settings.color.selected_text_style.prefix().to_string()
        } else {
            self.settings.color.text_style.prefix().to_string()
        }
    }

    /// Creates default string that represents one line from a menu
    fn create_string(
        &self,
        line: &str,
        description: Option<&str>,
        index: usize,
        row_number: &str,
        use_ansi_coloring: bool,
    ) -> String {
        let description = description.map_or("".to_string(), |desc| {
            if use_ansi_coloring {
                format!(
                    "{}({}) {}",
                    self.settings.color.description_style.prefix(),
                    desc,
                    RESET
                )
            } else {
                format!("({desc}) ")
            }
        });

        if use_ansi_coloring {
            format!(
                "{}{}{}{}{}{}",
                row_number,
                description,
                self.text_style(index),
                &line,
                RESET,
                Self::end_of_line(),
            )
        } else {
            // If no ansi coloring is found, then the selection word is
            // the line in uppercase
            let line_str = if index == self.index() {
                format!("{}{}>{}", row_number, description, line.to_uppercase())
            } else {
                format!("{row_number}{description}{line}")
            };

            // Final string with formatting
            format!("{}{}", line_str, Self::end_of_line())
        }
    }
}

impl Menu for ListMenu {
    /// Menu settings
    fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    /// Deactivates context menu
    fn is_active(&self) -> bool {
        self.active
    }

    /// There is no use for quick complete for the menu
    fn can_quick_complete(&self) -> bool {
        false
    }

    /// The menu should not try to auto complete to avoid comparing
    /// all registered values
    fn can_partially_complete(
        &mut self,
        _values_updated: bool,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
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

    /// Collecting the value from the completer to be shown in the menu
    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        let (input, pos) = completer_input(
            editor.get_buffer(),
            editor.insertion_point(),
            self.input.as_deref(),
            self.settings.only_buffer_difference,
        );

        let parsed = parse_selection_char(&input, SELECTION_CHAR);
        self.update_row_pos(parsed.index);

        // If there are no row selector and the menu has an Edit event, this clears
        // the position together with the pages vector
        if matches!(self.event, Some(MenuEvent::Edit(_))) && parsed.index.is_none() {
            self.reset_position();
        }

        self.values = if parsed.remainder.is_empty() {
            self.query_size = Some(completer.total_completions(parsed.remainder, pos));

            let skip = self.pages.iter().take(self.page).sum::<Page>().size;
            let take = self
                .pages
                .get(self.page)
                .map(|page| page.size)
                .unwrap_or(self.page_size);

            completer.partial_complete(&input, pos, skip, take)
        } else {
            self.query_size = None;
            completer.complete(&input, pos)
        }
    }

    /// Gets values from cached values that will be displayed in the menu
    fn get_values(&self) -> &[Suggestion] {
        if self.query_size.is_some() {
            // When there is a size value it means that only a chunk of the
            // chronological data from the database was collected
            &self.values
        } else {
            // If no record then it means that the values hold the result
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
    fn replace_in_buffer(&self, editor: &mut Editor) {
        replace_in_buffer(self.get_value(), editor);
    }

    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        if let Some(event) = self.event.clone() {
            match event {
                MenuEvent::Activate(_) => {
                    self.reset_position();

                    self.input = if self.settings.only_buffer_difference {
                        Some(editor.get_buffer().to_string())
                    } else {
                        None
                    };

                    self.update_values(editor, completer);

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
                    self.update_values(editor, completer);
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
                            self.update_working_details(editor, completer, painter);
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
                        self.update_working_details(editor, completer, painter);
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

                        self.update_values(editor, completer);
                        self.set_actual_page_size(self.printable_entries(painter));
                    } else {
                        self.row_position = 0;
                        self.page = 0;
                        self.update_values(editor, completer);
                    }
                }
                MenuEvent::PreviousPage => {
                    match self.page.checked_sub(1) {
                        Some(page_num) => self.page = page_num,
                        None => self.page = self.pages.len().saturating_sub(1),
                    }
                    self.update_values(editor, completer);
                }
            }

            self.event = None;
        }
    }

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal and if an entry is larger than the remaining lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        let mut entry_index = 0;
        self.get_values().iter().fold(0, |total_lines, suggestion| {
            //  to account for the the index and the indicator e.g. 0: XXXX
            let ret = total_lines
                + self.number_of_lines(
                    &suggestion.value,
                    terminal_columns.saturating_sub(
                        self.indicator().width() as u16 + count_digits(entry_index),
                    ),
                );
            entry_index += 1;
            ret
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
                            let lines = line.lines().take(self.max_lines as usize).fold(
                                String::new(),
                                |mut out_string, string| {
                                    let _ = write!(
                                        out_string,
                                        "{}\r\n{}",
                                        string, self.multiline_marker
                                    );
                                    out_string
                                },
                            );

                            lines + "..."
                        } else {
                            line.replace('\n', &format!("\r\n{}", self.multiline_marker))
                        };

                        let row_number = format!("{}: ", index + values_before_page);

                        self.create_string(
                            &line,
                            suggestion.description.as_deref(),
                            index,
                            &row_number,
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

fn count_digits(mut n: usize) -> u16 {
    // count the digits in the number
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    while n > 0 {
        n /= 10;
        count += 1;
    }
    count
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
