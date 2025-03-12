use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor,
    menu_functions::{can_partially_complete, completer_input, replace_in_buffer},
    painting::Painter,
    Completer, Suggestion,
};
use nu_ansi_term::ansi::RESET;
use unicode_width::UnicodeWidthStr;

/// Default values used as reference for the menu. These values are set during
/// the initial declaration of the menu and are always kept as reference for the
/// changeable [`ColumnDetails`]
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
    /// The shortest of the strings, which the suggestions are based on
    pub shortest_base_string: String,
}

/// Menu to present suggestions in a columnar fashion
/// It presents a description of the suggestion if available
pub struct ColumnarMenu {
    /// Menu settings
    settings: MenuSettings,
    /// Columnar menu active status
    active: bool,
    /// Default column details that are set when creating the menu
    /// These values are the reference for the working details
    default_details: DefaultColumnDetails,
    /// Number of minimum rows that are displayed when
    /// the required lines is larger than the available lines
    min_rows: u16,
    /// Working column details keep changing based on the collected values
    working_details: ColumnDetails,
    /// Menu cached values
    values: Vec<Suggestion>,
    /// column position of the cursor. Starts from 0
    col_pos: u16,
    /// row position in the menu. Starts from 0
    row_pos: u16,
    /// Event sent to the menu
    event: Option<MenuEvent>,
    /// Longest suggestion found in the values
    longest_suggestion: usize,
    /// String collected after the menu is activated
    input: Option<String>,
}

impl Default for ColumnarMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("columnar_menu"),
            active: false,
            default_details: DefaultColumnDetails::default(),
            min_rows: 3,
            working_details: ColumnDetails::default(),
            values: Vec::new(),
            col_pos: 0,
            row_pos: 0,
            event: None,
            longest_suggestion: 0,
            input: None,
        }
    }
}

// Menu configuration functions
impl MenuBuilder for ColumnarMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

// Menu specific configuration functions
impl ColumnarMenu {
    /// Menu builder with new columns value
    #[must_use]
    pub fn with_columns(mut self, columns: u16) -> Self {
        self.default_details.columns = columns;
        self
    }

    /// Menu builder with new column width value
    #[must_use]
    pub fn with_column_width(mut self, col_width: Option<usize>) -> Self {
        self.default_details.col_width = col_width;
        self
    }

    /// Menu builder with new column width value
    #[must_use]
    pub fn with_column_padding(mut self, col_padding: usize) -> Self {
        self.default_details.col_padding = col_padding;
        self
    }
}

// Menu functionality
impl ColumnarMenu {
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

    /// Move menu cursor up
    fn move_up(&mut self) {
        self.row_pos = if let Some(new_row) = self.row_pos.checked_sub(1) {
            new_row
        } else {
            let new_row = self.get_rows().saturating_sub(1);
            let index = new_row * self.get_cols() + self.col_pos;
            if index >= self.values.len() as u16 {
                new_row.saturating_sub(1)
            } else {
                new_row
            }
        }
    }

    /// Move menu cursor left
    fn move_down(&mut self) {
        let new_row = self.row_pos + 1;
        self.row_pos = if new_row >= self.get_rows() {
            0
        } else {
            let index = new_row * self.get_cols() + self.col_pos;
            if index >= self.values.len() as u16 {
                0
            } else {
                new_row
            }
        }
    }

    /// Move menu cursor left
    fn move_left(&mut self) {
        self.col_pos = if let Some(row) = self.col_pos.checked_sub(1) {
            row
        } else if self.index() + 1 == self.values.len() {
            0
        } else {
            self.get_cols().saturating_sub(1)
        }
    }

    /// Move menu cursor element
    fn move_right(&mut self) {
        let new_col = self.col_pos + 1;
        self.col_pos = if new_col >= self.get_cols() || self.index() + 2 > self.values.len() {
            0
        } else {
            new_col
        }
    }

    /// Menu index based on column and row position
    fn index(&self) -> usize {
        let index = self.row_pos * self.get_cols() + self.col_pos;
        index as usize
    }

    /// Get selected value from the menu
    fn get_value(&self) -> Option<Suggestion> {
        self.get_values().get(self.index()).cloned()
    }

    /// Calculates how many rows the Menu will use
    fn get_rows(&self) -> u16 {
        let values = self.get_values().len() as u16;

        if values == 0 {
            // When the values are empty the no_records_msg is shown, taking 1 line
            return 1;
        }

        let rows = values / self.get_cols();
        if values % self.get_cols() != 0 {
            rows + 1
        } else {
            rows
        }
    }

    /// Returns working details col width
    fn get_width(&self) -> usize {
        self.working_details.col_width
    }

    /// Reset menu position
    fn reset_position(&mut self) {
        self.col_pos = 0;
        self.row_pos = 0;
    }

    fn no_records_msg(&self, use_ansi_coloring: bool) -> String {
        let msg = "NO RECORDS FOUND";
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

    /// Returns working details columns
    fn get_cols(&self) -> u16 {
        self.working_details.columns.max(1)
    }

    /// End of line for menu
    fn end_of_line(&self, column: u16) -> &str {
        if column == self.get_cols().saturating_sub(1) {
            "\r\n"
        } else {
            ""
        }
    }

    /// Creates default string that represents one suggestion from the menu
    fn create_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
        column: u16,
        empty_space: usize,
        use_ansi_coloring: bool,
    ) -> String {
        if use_ansi_coloring {
            // strip quotes
            let is_quote = |c: char| "`'\"".contains(c);
            let shortest_base = &self.working_details.shortest_base_string;
            let shortest_base = shortest_base
                .strip_prefix(is_quote)
                .unwrap_or(shortest_base);
            let match_len = shortest_base.len();

            // Split string so the match text can be styled
            let skip_len = suggestion
                .value
                .chars()
                .take_while(|c| is_quote(*c))
                .count();
            let (match_str, remaining_str) = suggestion
                .value
                .split_at((match_len + skip_len).min(suggestion.value.len()));

            let suggestion_style_prefix = suggestion
                .style
                .unwrap_or(self.settings.color.text_style)
                .prefix();

            let left_text_size = self.longest_suggestion + self.default_details.col_padding;
            let right_text_size = self.get_width().saturating_sub(left_text_size);

            let max_remaining = left_text_size.saturating_sub(match_str.width());
            let max_match = max_remaining.saturating_sub(remaining_str.width());

            if index == self.index() {
                if let Some(description) = &suggestion.description {
                    format!(
                        "{}{}{}{}{}{:max_match$}{:max_remaining$}{}{}{}{}{}{}",
                        suggestion_style_prefix,
                        self.settings.color.selected_match_style.prefix(),
                        match_str,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        &remaining_str,
                        RESET,
                        self.settings.color.description_style.prefix(),
                        self.settings.color.selected_text_style.prefix(),
                        description
                            .chars()
                            .take(right_text_size)
                            .collect::<String>()
                            .replace('\n', " "),
                        RESET,
                        self.end_of_line(column),
                    )
                } else {
                    format!(
                        "{}{}{}{}{}{}{}{}{:>empty$}{}",
                        suggestion_style_prefix,
                        self.settings.color.selected_match_style.prefix(),
                        match_str,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        remaining_str,
                        RESET,
                        "",
                        self.end_of_line(column),
                        empty = empty_space,
                    )
                }
            } else if let Some(description) = &suggestion.description {
                format!(
                    "{}{}{}{}{:max_match$}{:max_remaining$}{}{}{}{}{}",
                    suggestion_style_prefix,
                    self.settings.color.match_style.prefix(),
                    match_str,
                    RESET,
                    suggestion_style_prefix,
                    remaining_str,
                    RESET,
                    self.settings.color.description_style.prefix(),
                    description
                        .chars()
                        .take(right_text_size)
                        .collect::<String>()
                        .replace('\n', " "),
                    RESET,
                    self.end_of_line(column),
                )
            } else {
                format!(
                    "{}{}{}{}{}{}{}{}{:>empty$}{}{}",
                    suggestion_style_prefix,
                    self.settings.color.match_style.prefix(),
                    match_str,
                    RESET,
                    suggestion_style_prefix,
                    remaining_str,
                    RESET,
                    self.settings.color.description_style.prefix(),
                    "",
                    RESET,
                    self.end_of_line(column),
                    empty = empty_space,
                )
            }
        } else {
            // If no ansi coloring is found, then the selection word is the line in uppercase
            let marker = if index == self.index() { ">" } else { "" };

            let line = if let Some(description) = &suggestion.description {
                format!(
                    "{}{:max$}{}{}",
                    marker,
                    &suggestion.value,
                    description
                        .chars()
                        .take(empty_space)
                        .collect::<String>()
                        .replace('\n', " "),
                    self.end_of_line(column),
                    max = self.longest_suggestion
                        + self
                            .default_details
                            .col_padding
                            .saturating_sub(marker.width()),
                )
            } else {
                format!(
                    "{}{}{:>empty$}{}",
                    marker,
                    &suggestion.value,
                    "",
                    self.end_of_line(column),
                    empty = empty_space.saturating_sub(marker.width()),
                )
            };

            if index == self.index() {
                line.to_uppercase()
            } else {
                line
            }
        }
    }
}

impl Menu for ColumnarMenu {
    /// Menu settings
    fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    /// Deactivates context menu
    fn is_active(&self) -> bool {
        self.active
    }

    /// The columnar menu can to quick complete if there is only one element
    fn can_quick_complete(&self) -> bool {
        true
    }

    /// The columnar menu can try to find the common string and replace it
    /// in the given line buffer
    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> bool {
        // If the values were already updated (e.g. quick completions are true)
        // there is no need to update the values from the menu
        if !values_updated {
            self.update_values(editor, completer);
        }

        if can_partially_complete(self.get_values(), editor) {
            // The values need to be updated because the spans need to be
            // recalculated for accurate replacement in the string
            self.update_values(editor, completer);

            true
        } else {
            false
        }
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

    /// Updates menu values
    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        let (input, pos) = completer_input(
            editor.get_buffer(),
            editor.insertion_point(),
            self.input.as_deref(),
            self.settings.only_buffer_difference,
        );

        let (values, base_ranges) = completer.complete_with_base_ranges(&input, pos);

        self.values = values;
        self.working_details.shortest_base_string = base_ranges
            .iter()
            .map(|range| editor.get_buffer()[range.clone()].to_string())
            .min_by_key(|s| s.width())
            .unwrap_or_default();

        self.reset_position();
    }

    /// The working details for the menu changes based on the size of the lines
    /// collected from the completer
    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        if let Some(event) = self.event.take() {
            match event {
                MenuEvent::Activate(updated) => {
                    self.active = true;
                    self.reset_position();

                    self.input = if self.settings.only_buffer_difference {
                        Some(editor.get_buffer().to_string())
                    } else {
                        None
                    };

                    if !updated {
                        self.update_values(editor, completer);
                    }
                }
                MenuEvent::Deactivate => self.active = false,
                MenuEvent::Edit(updated) => {
                    self.reset_position();

                    if !updated {
                        self.update_values(editor, completer);
                    }
                }
                MenuEvent::NextElement => self.move_next(),
                MenuEvent::PreviousElement => self.move_previous(),
                MenuEvent::MoveUp => self.move_up(),
                MenuEvent::MoveDown => self.move_down(),
                MenuEvent::MoveLeft => self.move_left(),
                MenuEvent::MoveRight => self.move_right(),
                MenuEvent::PreviousPage | MenuEvent::NextPage => {
                    // The columnar menu doest have the concept of pages, yet
                }
            }

            // The working value for the menu are updated only after executing the menu events,
            // so they have the latest suggestions
            //
            // If there is at least one suggestion that contains a description, then the layout
            // is changed to one column to fit the description
            let exist_description = self
                .get_values()
                .iter()
                .any(|suggestion| suggestion.description.is_some());

            if exist_description {
                self.working_details.columns = 1;
                self.working_details.col_width = painter.screen_width() as usize;

                self.longest_suggestion = self.get_values().iter().fold(0, |prev, suggestion| {
                    if prev >= suggestion.value.width() {
                        prev
                    } else {
                        suggestion.value.width()
                    }
                });
            } else {
                let max_width = self.get_values().iter().fold(0, |acc, suggestion| {
                    let str_len = suggestion.value.width() + self.default_details.col_padding;
                    if str_len > acc {
                        str_len
                    } else {
                        acc
                    }
                });

                // If no default width is found, then the total screen width is used to estimate
                // the column width based on the default number of columns
                let default_width = if let Some(col_width) = self.default_details.col_width {
                    col_width
                } else {
                    let col_width = painter.screen_width() / self.default_details.columns;
                    col_width as usize
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
                let possible_cols = painter.screen_width() / self.working_details.col_width as u16;
                if possible_cols > self.default_details.columns {
                    self.working_details.columns = self.default_details.columns.max(1);
                } else {
                    self.working_details.columns = possible_cols;
                }
            }
        }
    }

    /// The buffer gets replaced in the Span location
    fn replace_in_buffer(&self, editor: &mut Editor) {
        replace_in_buffer(self.get_value(), editor);
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.get_rows().min(self.min_rows)
    }

    /// Gets values from filler that will be displayed in the menu
    fn get_values(&self) -> &[Suggestion] {
        &self.values
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        self.get_rows()
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.get_values().is_empty() {
            self.no_records_msg(use_ansi_coloring)
        } else {
            // The skip values represent the number of lines that should be skipped
            // while printing the menu
            let skip_values = if self.row_pos >= available_lines {
                let skip_lines = self.row_pos.saturating_sub(available_lines) + 1;
                (skip_lines * self.get_cols()) as usize
            } else {
                0
            };

            // It seems that crossterm prefers to have a complete string ready to be printed
            // rather than looping through the values and printing multiple things
            // This reduces the flickering when printing the menu
            let available_values = (available_lines * self.get_cols()) as usize;
            self.get_values()
                .iter()
                .skip(skip_values)
                .take(available_values)
                .enumerate()
                .map(|(index, suggestion)| {
                    // Correcting the enumerate index based on the number of skipped values
                    let index = index + skip_values;
                    let column = index as u16 % self.get_cols();
                    let empty_space = self.get_width().saturating_sub(suggestion.value.width());

                    self.create_string(suggestion, index, column, empty_space, use_ansi_coloring)
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Span, UndoBehavior};

    use super::*;

    macro_rules! partial_completion_tests {
        (name: $test_group_name:ident, completions: $completions:expr, test_cases: $($name:ident: $value:expr,)*) => {
            mod $test_group_name {
                use crate::{menu::Menu, ColumnarMenu, core_editor::Editor, enums::UndoBehavior};
                use super::FakeCompleter;

                $(
                    #[test]
                    fn $name() {
                        let (input, expected) = $value;
                        let mut menu = ColumnarMenu::default();
                        let mut editor = Editor::default();
                        editor.set_buffer(input.to_string(), UndoBehavior::CreateUndoPoint);
                        let mut completer = FakeCompleter::new(&$completions);

                        menu.can_partially_complete(false, &mut editor, &mut completer);

                        assert_eq!(editor.get_buffer(), expected);
                    }
                )*
            }
        }
    }

    partial_completion_tests! {
        name: partial_completion_prefix_matches,
        completions: ["build.rs", "build-all.sh"],

        test_cases:
            empty_completes_prefix: ("", "build"),
            partial_completes_shared_prefix: ("bui", "build"),
            full_prefix_completes_nothing: ("build", "build"),
    }

    partial_completion_tests! {
        name: partial_completion_fuzzy_matches,
        completions: ["build.rs", "build-all.sh", "prepare-build.sh"],

        test_cases:
            no_shared_prefix_completes_nothing: ("", ""),
            shared_prefix_completes_nothing: ("bui", "bui"),
    }

    partial_completion_tests! {
        name: partial_completion_fuzzy_same_prefix_matches,
        completions: ["build.rs", "build-all.sh", "build-all-tests.sh"],

        test_cases:
            // assure "all" does not get replaced with shared prefix "build"
            completes_no_shared_prefix: ("all", "all"),
    }

    struct FakeCompleter {
        completions: Vec<String>,
    }

    impl FakeCompleter {
        fn new(completions: &[&str]) -> Self {
            Self {
                completions: completions.iter().map(|c| c.to_string()).collect(),
            }
        }
    }

    impl Completer for FakeCompleter {
        fn complete(&mut self, _line: &str, pos: usize) -> Vec<Suggestion> {
            self.completions
                .iter()
                .map(|c| fake_suggestion(c, pos))
                .collect()
        }
    }

    fn fake_suggestion(name: &str, pos: usize) -> Suggestion {
        Suggestion {
            value: name.to_string(),
            description: None,
            style: None,
            extra: None,
            span: Span { start: 0, end: pos },
            append_whitespace: false,
        }
    }

    #[test]
    fn test_menu_replace_backtick() {
        // https://github.com/nushell/nushell/issues/7885
        let mut completer = FakeCompleter::new(&["file1.txt", "file2.txt"]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();

        // backtick at the end of the line
        editor.set_buffer("file1.txt`".to_string(), UndoBehavior::CreateUndoPoint);

        menu.update_values(&mut editor, &mut completer);

        menu.replace_in_buffer(&mut editor);

        // After replacing the editor, make sure insertion_point is at the right spot
        assert!(
            editor.is_cursor_at_buffer_end(),
            "cursor should be at the end after completion"
        );
    }

    #[test]
    fn test_menu_create_string() {
        // https://github.com/nushell/nushell/issues/13951
        let mut completer = FakeCompleter::new(&["おはよう", "`おはよう(`"]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();

        editor.set_buffer("おは".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.menu_string(2, true).contains("`おは"));
    }
}
