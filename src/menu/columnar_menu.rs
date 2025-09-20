use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor,
    menu_functions::{
        can_partially_complete, completer_input, floor_char_boundary, replace_in_buffer,
    },
    painting::Painter,
    Completer, Suggestion,
};
use nu_ansi_term::ansi::RESET;
use unicode_width::UnicodeWidthStr;

/// The traversal direction of the menu
#[derive(Debug, PartialEq, Eq)]
pub enum TraversalDirection {
    /// Traverse horizontally
    Horizontal,
    /// Traverse vertically
    Vertical,
}

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
    /// Traversal direction
    pub traversal_dir: TraversalDirection,
}

impl Default for DefaultColumnDetails {
    fn default() -> Self {
        Self {
            columns: 4,
            col_width: None,
            col_padding: 2,
            traversal_dir: TraversalDirection::Horizontal,
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
    /// Number of rows that are skipped when printing,
    /// depending on selected value and terminal height
    skip_rows: u16,
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
            skip_rows: 0,
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

    /// Menu builder with new traversal direction value
    #[must_use]
    pub fn with_traversal_direction(mut self, direction: TraversalDirection) -> Self {
        self.default_details.traversal_dir = direction;
        self
    }
}

// Menu functionality
impl ColumnarMenu {
    /// Move menu cursor to the next element
    fn move_next(&mut self) {
        let new_index = self.index() + 1;

        let new_index = if new_index >= self.get_values().len() {
            0
        } else {
            new_index
        };

        (self.row_pos, self.col_pos) = self.position_from_index(new_index);
    }

    /// Move menu cursor to the previous element
    fn move_previous(&mut self) {
        let new_index = match self.index().checked_sub(1) {
            Some(index) => index,
            None => self.values.len().saturating_sub(1),
        };

        (self.row_pos, self.col_pos) = self.position_from_index(new_index);
    }

    /// Move menu cursor up
    fn move_up(&mut self) {
        self.row_pos = match self.row_pos.checked_sub(1) {
            Some(index) => index,
            None => self.get_last_row_at_col(self.col_pos),
        }
    }

    /// Move menu cursor down
    fn move_down(&mut self) {
        let new_row = self.row_pos + 1;
        self.row_pos = if new_row > self.get_last_row_at_col(self.col_pos) {
            0
        } else {
            new_row
        }
    }

    /// Move menu cursor left
    fn move_left(&mut self) {
        self.col_pos = if let Some(col) = self.col_pos.checked_sub(1) {
            col
        } else {
            self.get_last_col_at_row(self.row_pos)
        }
    }

    /// Move menu cursor right
    fn move_right(&mut self) {
        let new_col = self.col_pos + 1;
        self.col_pos = if new_col > self.get_last_col_at_row(self.row_pos) {
            0
        } else {
            new_col
        }
    }

    /// Calculates row and column positions from an index
    fn position_from_index(&self, index: usize) -> (u16, u16) {
        match self.default_details.traversal_dir {
            TraversalDirection::Vertical => {
                let row = index % self.get_rows() as usize;
                let col = index / self.get_rows() as usize;
                (row as u16, col as u16)
            }
            TraversalDirection::Horizontal => {
                let row = index / self.get_used_cols() as usize;
                let col = index % self.get_used_cols() as usize;
                (row as u16, col as u16)
            }
        }
    }

    /// Calculates the last row containing a value for the specified column
    fn get_last_row_at_col(&self, col_pos: u16) -> u16 {
        let num_values = self.get_values().len() as u16;
        match self.default_details.traversal_dir {
            TraversalDirection::Vertical => {
                if col_pos >= self.get_used_cols() - 1 {
                    // Last column, might not be full
                    let mod_val = num_values % self.get_rows();
                    if mod_val == 0 {
                        // Full column
                        self.get_rows().saturating_sub(1)
                    } else {
                        // Column with last row empty
                        mod_val.saturating_sub(1)
                    }
                } else {
                    // Full column
                    self.get_rows().saturating_sub(1)
                }
            }
            TraversalDirection::Horizontal => {
                let mod_val = num_values % self.get_used_cols();
                if mod_val > 0 && col_pos >= mod_val {
                    // Column with last row empty
                    self.get_rows().saturating_sub(2)
                } else {
                    // Full column
                    self.get_rows().saturating_sub(1)
                }
            }
        }
    }

    /// Calculates the last column containing a value for the specified row
    fn get_last_col_at_row(&self, row_pos: u16) -> u16 {
        let num_values = self.get_values().len() as u16;
        match self.default_details.traversal_dir {
            TraversalDirection::Vertical => {
                let mod_val = num_values % self.get_rows();
                if mod_val > 0 && row_pos >= mod_val {
                    // Row with last column empty
                    self.get_used_cols().saturating_sub(2)
                } else {
                    // Full row
                    self.get_used_cols().saturating_sub(1)
                }
            }
            TraversalDirection::Horizontal => {
                if row_pos >= self.get_rows() - 1 {
                    // Last row, might not be full
                    let mod_val = num_values % self.get_used_cols();
                    if mod_val == 0 {
                        // Full row
                        self.get_used_cols().saturating_sub(1)
                    } else {
                        // Row with some columns empty
                        mod_val.saturating_sub(1)
                    }
                } else {
                    // Full row
                    self.get_used_cols().saturating_sub(1)
                }
            }
        }
    }

    /// Menu index based on column and row position
    fn index(&self) -> usize {
        let index = match self.default_details.traversal_dir {
            TraversalDirection::Vertical => self.col_pos * self.get_rows() + self.row_pos,
            TraversalDirection::Horizontal => self.row_pos * self.get_used_cols() + self.col_pos,
        };
        index.into()
    }

    /// Get selected value from the menu
    fn get_value(&self) -> Option<Suggestion> {
        self.get_values().get(self.index()).cloned()
    }

    /// Calculates how many rows the menu will use
    fn get_rows(&self) -> u16 {
        let values = self.get_values().len() as u16;

        if values == 0 {
            // When the values are empty the "NO RECORDS FOUND" message is shown, taking 1 line
            return 1;
        }

        let rows = values / self.get_cols();
        if values % self.get_cols() != 0 {
            rows + 1
        } else {
            rows
        }
    }

    /// Calculates how many columns will be used to display values
    fn get_used_cols(&self) -> u16 {
        let values = self.get_values().len() as u16;

        if values == 0 {
            // When the values are empty the "NO RECORDS FOUND" message is shown, taking 1 column
            return 1;
        }

        match self.default_details.traversal_dir {
            TraversalDirection::Vertical => {
                let cols = values / self.get_rows();
                if values % self.get_rows() != 0 {
                    cols + 1
                } else {
                    cols
                }
            }
            TraversalDirection::Horizontal => self.get_cols().min(values),
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

    /// Creates default string that represents one suggestion from the menu
    fn create_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
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
            let match_len = shortest_base.chars().count();

            // Find match position - look for the base string in the suggestion (case-insensitive)
            let match_position = suggestion
                .value
                .to_lowercase()
                .find(&shortest_base.to_lowercase())
                .unwrap_or(0);

            // The match is just the part that matches the shortest_base
            let match_str = {
                let match_str = &suggestion.value[match_position..];
                let match_len_bytes = match_str
                    .char_indices()
                    .nth(match_len)
                    .map(|(i, _)| i)
                    .unwrap_or_else(|| match_str.len());
                &suggestion.value[match_position..match_position + match_len_bytes]
            };

            // Prefix is everything before the match
            let prefix = &suggestion.value[..match_position];

            // Remaining is everything after the match
            let remaining_str = &suggestion.value[match_position + match_str.len()..];

            let suggestion_style_prefix = suggestion
                .style
                .unwrap_or(self.settings.color.text_style)
                .prefix();

            let left_text_size = self.longest_suggestion + self.default_details.col_padding;
            let right_text_size = self.get_width().saturating_sub(left_text_size);

            let max_remaining = left_text_size.saturating_sub(match_str.width() + prefix.width());
            let max_match = max_remaining.saturating_sub(remaining_str.width());

            if index == self.index() {
                if let Some(description) = &suggestion.description {
                    format!(
                        "{}{}{}{}{}{}{}{}{}{:max_match$}{:max_remaining$}{}{}{}{}{}",
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        prefix,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_match_style.prefix(),
                        match_str,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        remaining_str,
                        RESET,
                        self.settings.color.description_style.prefix(),
                        self.settings.color.selected_text_style.prefix(),
                        description
                            .chars()
                            .take(right_text_size)
                            .collect::<String>()
                            .replace('\n', " "),
                        RESET,
                    )
                } else {
                    format!(
                        "{}{}{}{}{}{}{}{}{}{}{}{}{:>empty$}",
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        prefix,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_match_style.prefix(),
                        match_str,
                        RESET,
                        suggestion_style_prefix,
                        self.settings.color.selected_text_style.prefix(),
                        remaining_str,
                        RESET,
                        "",
                        empty = empty_space,
                    )
                }
            } else if let Some(description) = &suggestion.description {
                format!(
                    "{}{}{}{}{}{}{}{:max_match$}{:max_remaining$}{}{}{}{}",
                    suggestion_style_prefix,
                    prefix,
                    RESET,
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
                )
            } else {
                format!(
                    "{}{}{}{}{}{}{}{}{}{}{}{:>empty$}{}",
                    suggestion_style_prefix,
                    prefix,
                    RESET,
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
                    empty = empty_space,
                )
            }
        } else {
            // If no ansi coloring is found, then the selection word is the line in uppercase
            let marker = if index == self.index() { ">" } else { "" };

            let line = if let Some(description) = &suggestion.description {
                format!(
                    "{}{:max$}{}",
                    marker,
                    &suggestion.value,
                    description
                        .chars()
                        .take(empty_space)
                        .collect::<String>()
                        .replace('\n', " "),
                    max = self.longest_suggestion
                        + self
                            .default_details
                            .col_padding
                            .saturating_sub(marker.width()),
                )
            } else {
                format!(
                    "{}{}{:>empty$}",
                    marker,
                    &suggestion.value,
                    "",
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
            .map(|range| {
                let end = floor_char_boundary(editor.get_buffer(), range.end);
                let start = floor_char_boundary(editor.get_buffer(), range.start).min(end);
                editor.get_buffer()[start..end].to_string()
            })
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

            let mut available_lines = painter.remaining_lines_real();
            // Handle the case where a prompt uses the entire screen.
            // Drawing the menu has priority over the drawing the prompt.
            if available_lines == 0 {
                available_lines = painter.remaining_lines().min(self.min_rows());
            }

            self.skip_rows = if self.row_pos < self.skip_rows {
                // Selection is above the visible area, scroll up
                self.row_pos
            } else if self.row_pos >= self.skip_rows + available_lines {
                // Selection is below the visible area, scroll down
                self.row_pos - available_lines + 1
            } else {
                // Selection is within the visible area
                self.skip_rows
            };
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
            // It seems that crossterm prefers to have a complete string ready to be printed
            // rather than looping through the values and printing multiple things
            // This reduces the flickering when printing the menu
            match self.default_details.traversal_dir {
                TraversalDirection::Vertical => {
                    let num_rows: usize = self.get_rows().into();
                    let rows_to_draw = num_rows.min(available_lines.into());
                    let mut menu_string = String::new();
                    for line in 0..rows_to_draw {
                        let skip_value = self.skip_rows as usize + line;
                        let row_string: String = self
                            .get_values()
                            .iter()
                            .enumerate()
                            .skip(skip_value)
                            .step_by(num_rows)
                            .take(self.get_cols().into())
                            .map(|(index, suggestion)| {
                                let empty_space =
                                    self.get_width().saturating_sub(suggestion.value.width());
                                self.create_string(
                                    suggestion,
                                    index,
                                    empty_space,
                                    use_ansi_coloring,
                                )
                            })
                            .collect();
                        menu_string.push_str(&row_string);
                        menu_string.push_str("\r\n");
                    }
                    menu_string
                }
                TraversalDirection::Horizontal => {
                    let available_values = (available_lines * self.get_cols()) as usize;
                    let skip_values = (self.skip_rows * self.get_used_cols()) as usize;

                    self.get_values()
                        .iter()
                        .skip(skip_values)
                        .take(available_values)
                        .enumerate()
                        .map(|(index, suggestion)| {
                            // Correcting the enumerate index based on the number of skipped values
                            let index = index + skip_values;
                            let column = index % self.get_cols() as usize;
                            let empty_space =
                                self.get_width().saturating_sub(suggestion.value.width());

                            let end_of_line =
                                if column == self.get_cols().saturating_sub(1) as usize {
                                    "\r\n"
                                } else {
                                    ""
                                };
                            format!(
                                "{}{}",
                                self.create_string(
                                    suggestion,
                                    index,
                                    empty_space,
                                    use_ansi_coloring
                                ),
                                end_of_line
                            )
                        })
                        .collect()
                }
            }
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
        assert!(menu.menu_string(2, true).contains("おは"));
    }

    #[test]
    fn test_menu_create_string_starting_with_multibyte_char() {
        // https://github.com/nushell/nushell/issues/15938
        let mut completer = FakeCompleter::new(&["验abc/"]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();

        editor.set_buffer("ac".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.menu_string(10, true).contains("验"));
    }

    #[test]
    fn test_menu_create_string_long_unicode_string() {
        // Test for possible panic if a long filename gets truncated
        let mut completer = FakeCompleter::new(&[&("验".repeat(205) + "abc/")]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();

        editor.set_buffer("a".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.menu_string(10, true).contains("验"));
    }

    #[test]
    fn test_horizontal_menu_selection_position() {
        // Test selection position update
        let vs: Vec<String> = (0..10).map(|v| v.to_string()).collect();
        let vs: Vec<_> = vs.iter().map(|v| v.as_ref()).collect();
        let mut completer = FakeCompleter::new(&vs);
        let mut menu = ColumnarMenu::default()
            .with_traversal_direction(TraversalDirection::Horizontal)
            .with_name("testmenu");
        menu.working_details.columns = 4;
        let mut editor = Editor::default();

        editor.set_buffer("a".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.index() == 0);
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Next/previous wrapping
        menu.move_previous();
        assert!(menu.index() == vs.len() - 1);
        assert!(menu.row_pos == 2 && menu.col_pos == 1);
        menu.move_next();
        assert!(menu.index() == 0);
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Up/down/left/right wrapping for full rows/columns
        menu.move_up();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
        menu.move_down();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        menu.move_left();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_right();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Up/down/left/right wrapping for non-full rows/columns
        menu.move_left();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_up();
        assert!(menu.row_pos == 1 && menu.col_pos == 3);
        menu.move_down();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_right();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        menu.move_up();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
        menu.move_left();
        assert!(menu.row_pos == 2 && menu.col_pos == 1);
        menu.move_right();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
    }

    #[test]
    fn test_vertical_menu_selection_position() {
        // Test selection position update
        let vs: Vec<String> = (0..11).map(|v| v.to_string()).collect();
        let vs: Vec<_> = vs.iter().map(|v| v.as_ref()).collect();
        let mut completer = FakeCompleter::new(&vs);
        let mut menu = ColumnarMenu::default()
            .with_traversal_direction(TraversalDirection::Vertical)
            .with_name("testmenu");
        menu.working_details.columns = 4;
        let mut editor = Editor::default();

        editor.set_buffer("a".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.index() == 0);
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Next/previous wrapping
        menu.move_previous();
        assert!(menu.index() == vs.len() - 1);
        assert!(menu.row_pos == 1 && menu.col_pos == 3);
        menu.move_next();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Up/down/left/right wrapping for full rows/columns
        menu.move_up();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
        menu.move_down();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        menu.move_left();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_right();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        // Up/down/left/right wrapping for non-full rows/columns
        menu.move_left();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_up();
        assert!(menu.row_pos == 1 && menu.col_pos == 3);
        menu.move_down();
        assert!(menu.row_pos == 0 && menu.col_pos == 3);
        menu.move_right();
        assert!(menu.row_pos == 0 && menu.col_pos == 0);
        menu.move_up();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
        menu.move_left();
        assert!(menu.row_pos == 2 && menu.col_pos == 2);
        menu.move_right();
        assert!(menu.row_pos == 2 && menu.col_pos == 0);
    }

    #[test]
    fn test_small_menu_selection_position() {
        // Test selection position update for menus with fewer values than available columns
        let mut vertical_menu = ColumnarMenu::default()
            .with_traversal_direction(TraversalDirection::Vertical)
            .with_name("testmenu");
        vertical_menu.working_details.columns = 4;
        let mut horizontal_menu = ColumnarMenu::default()
            .with_traversal_direction(TraversalDirection::Horizontal)
            .with_name("testmenu");
        horizontal_menu.working_details.columns = 4;
        let mut editor = Editor::default();

        let mut completer = FakeCompleter::new(&["1", "2"]);

        for menu in &mut [vertical_menu, horizontal_menu] {
            menu.update_values(&mut editor, &mut completer);
            assert!(menu.index() == 0);
            assert!(menu.row_pos == 0 && menu.col_pos == 0);
            menu.move_previous();
            assert!(menu.index() == menu.get_values().len() - 1);
            assert!(menu.row_pos == 0 && menu.col_pos == 1);
            menu.move_next();
            assert!(menu.row_pos == 0 && menu.col_pos == 0);
            menu.move_next();
            assert!(menu.row_pos == 0 && menu.col_pos == 1);
            menu.move_right();
            assert!(menu.row_pos == 0 && menu.col_pos == 0);
            menu.move_left();
            assert!(menu.row_pos == 0 && menu.col_pos == 1);
            menu.move_up();
            assert!(menu.row_pos == 0 && menu.col_pos == 1);
            menu.move_down();
            assert!(menu.row_pos == 0 && menu.col_pos == 1);
        }
    }
}
