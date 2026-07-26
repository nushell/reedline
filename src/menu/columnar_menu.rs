use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor,
    menu_functions::{
        available_lines, can_partially_complete, get_match_indices, replace_in_buffer,
        resolve_completer_input, scroll_offset, style_suggestion, truncate_with_ansi,
        CompletionDisplay,
    },
    painting::Painter,
    Completer, Suggestion,
};
use nu_ansi_term::ansi::RESET;

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
    /// Suggestions currently displayed and their derived display metrics
    completions: CompletionDisplay,
    /// Whether a background completion is still in flight with nothing to show
    /// yet. While set, the menu draws nothing rather than a premature
    /// "NO RECORDS FOUND" (which is reserved for a settled, genuinely empty result).
    awaiting_results: bool,
    /// column position of the cursor. Starts from 0
    col_pos: u16,
    /// row position in the menu. Starts from 0
    row_pos: u16,
    /// Number of rows that are skipped when printing,
    /// depending on selected value and terminal height
    skip_rows: u16,
    /// Event sent to the menu
    event: Option<MenuEvent>,
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
            completions: CompletionDisplay::default(),
            awaiting_results: false,
            col_pos: 0,
            row_pos: 0,
            skip_rows: 0,
            event: None,
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
            None => self.completions.values.len().saturating_sub(1),
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
        match self.get_values().len() as u16 {
            // No reason to save space if we're waiting for results.
            0 if self.awaiting_results => 0,
            // Should be one row for actual empty results
            0 => 1,
            total_values => (total_values + self.get_cols() - 1) / self.get_cols(),
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
    pub fn create_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
        use_ansi_coloring: bool,
    ) -> String {
        match use_ansi_coloring {
            true => self.format_ansi(suggestion, index),
            false => self.format_plain(suggestion, index),
        }
    }

    /// Horizontal split of a menu row into the suggestion column and the
    /// description that follows it.
    fn column_split(&self, terminal_width: usize) -> (usize, usize) {
        let maximum_left_size =
            self.completions.longest_suggestion + self.default_details.col_padding;
        let left_text_size = terminal_width.min(maximum_left_size);
        let description_size = terminal_width.saturating_sub(left_text_size);

        (left_text_size, description_size)
    }

    /// Handles plain-text formatting.
    fn format_plain(&self, suggestion: &Suggestion, index: usize) -> String {
        let is_selected = index == self.index();
        let terminal_width = self.get_width();
        let display_value = suggestion.display_value();

        // Calculate the remaining space after the suggestion text
        let empty_space = terminal_width.saturating_sub(self.completions.display_widths[index]);
        let marker = if is_selected { ">" } else { "" };
        let description = suggestion.description.as_deref().unwrap_or("");

        let mut formatted_line = if description.is_empty() {
            // If there is no description, pad with empty space to fill the terminal width.
            let padding_width = empty_space.saturating_sub(marker.len());
            format!("{marker}{display_value}{:padding_width$}", "")
        } else {
            // The left column is capped at `left_text_size` and the description
            // takes only what's left, so the row can't exceed the terminal
            let (left_text_size, description_size) = self.column_split(terminal_width);
            let padding_width = left_text_size.saturating_sub(marker.len());
            let mut base_line = format!("{marker}{display_value:padding_width$}");

            base_line.extend(description.chars().take(description_size).map(|character| {
                if character == '\n' {
                    ' '
                } else {
                    character
                }
            }));
            base_line
        };

        if is_selected {
            // Modifies in-place. Prevents unicode edge cases as well.
            formatted_line.make_ascii_uppercase();
        }

        formatted_line
    }

    /// Handles ANSI-colored formatting.
    fn format_ansi(&self, suggestion: &Suggestion, index: usize) -> String {
        let is_selected = index == self.index();
        let terminal_width = self.get_width();
        let display_value = suggestion.display_value();
        let display_width = self.completions.display_widths[index];
        let color_settings = &self.settings.color;

        // TODO(ysthakur): let the user strip quotes, rather than doing it here
        // Natively trim standard quote characters using a character array match.
        let base_string = self
            .completions
            .shortest_base_string
            .trim_start_matches(['`', '\'', '"']);
        let match_indices =
            get_match_indices(display_value, &suggestion.match_indices, base_string);

        // Calculate spatial boundaries for the suggestion text and its description.
        let (left_text_size, description_size) = self.column_split(terminal_width);

        // Resolve ANSI styles based on selection state.
        let text_style = suggestion
            .style
            .as_ref()
            .unwrap_or(&color_settings.text_style);
        let match_style = if is_selected {
            &color_settings.selected_match_style
        } else {
            &color_settings.match_style
        };

        let selected_style = is_selected.then_some(&color_settings.selected_text_style);

        // Truncate and apply ANSI highlighting to the primary suggestion text.
        let truncated_value = truncate_with_ansi(display_value, left_text_size);
        let styled_value = style_suggestion(
            &truncated_value,
            &match_indices,
            text_style,
            match_style,
            selected_style,
        );

        // Extract the description, but only if it's long enough
        // to be visible (> 3).
        let description = suggestion
            .description
            .as_deref()
            .filter(|_| description_size > 3)
            .unwrap_or("");

        if description.is_empty() {
            let padding_width = terminal_width.saturating_sub(display_width);
            return format!("{styled_value}{RESET}{:padding_width$}", "");
        }

        // Prepare the description by filtering out newlines
        // and applying truncation and paint.
        let padding_size = left_text_size.saturating_sub(display_width);
        let cleaned_description: String = description
            .chars()
            .filter(|&character| character != '\n')
            .collect();
        let painted_description = color_settings
            .description_style
            .paint(truncate_with_ansi(&cleaned_description, description_size));

        match is_selected {
            true => format!(
                "{styled_value}{RESET}{}{}{:padding_size$}{painted_description}{RESET}",
                text_style.prefix(),
                color_settings.selected_text_style.prefix(),
                ""
            ),
            false => format!(
                "{styled_value}{:padding_size$}{RESET}{painted_description}{RESET}",
                ""
            ),
        }
    }

    /// Apply a queued menu event, refreshing the values or moving the selection
    fn apply_event(
        &mut self,
        event: MenuEvent,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) {
        match event {
            MenuEvent::Activate(updated) | MenuEvent::Edit(updated) => {
                self.reload(updated, editor, completer)
            }
            MenuEvent::Deactivate => {}
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
    }

    /// Recompute the completion box geometry from the current suggestions,
    /// selection, and terminal size. Runs on every repaint.
    pub fn recompute_layout(&mut self, painter: &Painter) {
        let screen_width = painter.screen_width() as usize;

        // If there is at least one suggestion that contains a description, then the layout
        // is changed to one column to fit the description
        let has_descriptions = self
            .get_values()
            .iter()
            .any(|suggestion| suggestion.description.is_some());

        if has_descriptions {
            self.working_details.columns = 1;
            self.working_details.col_width = screen_width;
        } else {
            // If no default width is found, then the total screen width is used to estimate
            // the column width based on the default number of columns
            let default_width = self
                .default_details
                .col_width
                .unwrap_or_else(|| screen_width / self.default_details.columns as usize);

            // Adjusting the working width of the column based the max line width found
            // in the menu values
            let required_width =
                self.completions.longest_suggestion + self.default_details.col_padding;

            self.working_details.col_width = required_width.max(default_width).min(screen_width);

            // The working columns is adjusted based on possible number of columns
            // that could be fitted in the screen with the calculated column width
            let possible_columns = (screen_width / self.working_details.col_width) as u16;
            self.working_details.columns =
                possible_columns.min(self.default_details.columns).max(1);
        }

        let available = available_lines(painter, self.min_rows(), u16::MAX);
        self.skip_rows = scroll_offset(self.row_pos, self.skip_rows, available);
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
    fn reset_position(&mut self) {
        self.col_pos = 0;
        self.row_pos = 0;
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        let (input, pos) = resolve_completer_input(editor, &mut self.input, &self.settings);

        let (result, base_ranges) = completer.complete_with_base_ranges(&input, pos);
        self.awaiting_results = result.is_pending();
        if let Some(completions) = CompletionDisplay::from_result(result, &base_ranges, editor) {
            self.completions = completions;
            self.reset_position();
        }
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
            self.apply_event(event, editor, completer);
        }

        self.recompute_layout(painter);
    }

    /// The buffer gets replaced in the Span location
    fn replace_in_buffer(&self, editor: &mut Editor) {
        replace_in_buffer(self.get_value(), editor, self.settings.output_mode);
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.get_rows().min(self.min_rows)
    }

    /// Gets values from filler that will be displayed in the menu
    fn get_values(&self) -> &[Suggestion] {
        &self.completions.values
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        self.get_rows()
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.get_values().is_empty() {
            if self.awaiting_results {
                // A background completion is still running; draw nothing rather
                // than flashing "NO RECORDS FOUND" before the results land.
                String::new()
            } else {
                self.no_records_msg(use_ansi_coloring)
            }
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
                                self.create_string(suggestion, index, use_ansi_coloring)
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

                            let end_of_line =
                                if column == self.get_cols().saturating_sub(1) as usize {
                                    "\r\n"
                                } else {
                                    ""
                                };
                            format!(
                                "{}{}",
                                self.create_string(suggestion, index, use_ansi_coloring),
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
    use crate::painting::W;

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

    // https://github.com/nushell/nushell/issues/15535
    partial_completion_tests! {
        name: partial_completion_with_quotes,
        completions: ["`Foo bar`", "`Foo baz`"],

        test_cases:
            partial_completes_prefix_with_backtick: ("F", "`Foo ba"),
            partial_completes_case_insensitive: ("foo", "`Foo ba"),
    }

    partial_completion_tests! {
        name: partial_completion_unicode_case_folding,
        completions: ["ßar", "ßaz"],

        test_cases:
            partial_completes_case_insensitive: ("ss", "ßa"),
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
        fn complete(&mut self, _line: &str, pos: usize) -> crate::CompletionResult {
            crate::CompletionResult::fresh(
                self.completions
                    .iter()
                    .map(|c| fake_suggestion(c, pos))
                    .collect::<Vec<_>>(),
            )
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
            ..Default::default()
        }
    }

    /// Like [`FakeCompleter`] but every suggestion carries a description, which
    /// drives the columnar menu into its single-column layout.
    struct DescribedCompleter {
        completions: Vec<String>,
    }

    impl DescribedCompleter {
        fn new(completions: &[&str]) -> Self {
            Self {
                completions: completions.iter().map(|c| c.to_string()).collect(),
            }
        }
    }

    impl Completer for DescribedCompleter {
        fn complete(&mut self, _line: &str, pos: usize) -> crate::CompletionResult {
            crate::CompletionResult::fresh(
                self.completions
                    .iter()
                    .map(|c| {
                        let mut suggestion = fake_suggestion(c, pos);
                        suggestion.description = Some(format!("desc for {c}"));
                        suggestion
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }

    fn setup_menu(
        menu: &mut ColumnarMenu,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        terminal_size: (u16, u16),
    ) {
        let mut painter = Painter::new(W::sink());
        painter.handle_resize(terminal_size.0, terminal_size.1);

        menu.menu_event(MenuEvent::Activate(false));
        menu.update_working_details(editor, completer, &painter);
    }

    /// The menu layout must recompute whenever `update_working_details` runs,
    /// not only when a menu event is queued. Here the suggestions are refreshed
    /// through `update_values` (as a repaint would after new results arrive)
    /// with no pending event; the appearance of descriptions must still collapse
    /// the layout to a single column instead of waiting for the next keypress.
    #[test]
    fn layout_recomputes_without_a_menu_event() {
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();
        let mut painter = Painter::new(W::sink());
        painter.handle_resize(120, 10);

        // Initial results have no descriptions -> normal multi-column layout.
        let mut plain = FakeCompleter::new(&["alpha", "beta", "gamma"]);
        menu.menu_event(MenuEvent::Activate(false));
        menu.update_working_details(&mut editor, &mut plain, &painter);
        assert!(
            menu.get_cols() > 1,
            "expected a multi-column layout without descriptions"
        );

        // Refresh the values without queuing a menu event, then repaint.
        let mut described = DescribedCompleter::new(&["alpha", "beta", "gamma"]);
        menu.update_values(&mut editor, &mut described);
        assert!(menu.event.is_none(), "no menu event should be queued");
        menu.update_working_details(&mut editor, &mut described, &painter);

        // Descriptions must collapse the menu to one column now; before the fix
        // this only took effect after the next menu event (e.g. an arrow key).
        assert_eq!(
            menu.get_cols(),
            1,
            "descriptions must relayout to one column without a menu event"
        );
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
        setup_menu(&mut menu, &mut editor, &mut completer, (10, 10));

        assert!(menu.menu_string(2, true).contains("おは"));
    }

    #[test]
    fn test_menu_create_string_starting_with_multibyte_char() {
        // https://github.com/nushell/nushell/issues/15938
        let mut completer = FakeCompleter::new(&["验abc/"]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();
        editor.set_buffer("ac".to_string(), UndoBehavior::CreateUndoPoint);
        setup_menu(&mut menu, &mut editor, &mut completer, (10, 10));

        assert!(menu.menu_string(2, true).contains("验"));
    }

    #[test]
    fn test_menu_create_string_long_unicode_string() {
        // Test for possible panic if a long filename gets truncated
        let mut completer = FakeCompleter::new(&[&("验".repeat(205) + "abc/")]);
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();
        editor.set_buffer("a".to_string(), UndoBehavior::CreateUndoPoint);
        setup_menu(&mut menu, &mut editor, &mut completer, (10, 10));

        assert!(menu.menu_string(10, true).contains("验"));
    }

    /// A completer whose result is always `Pending` (a background compute that
    /// never lands within the test), to exercise the awaiting-results path.
    struct PendingCompleter;

    impl Completer for PendingCompleter {
        fn complete(&mut self, _line: &str, _pos: usize) -> crate::CompletionResult {
            crate::CompletionResult::Pending
        }
    }

    #[test]
    fn pending_completion_draws_nothing_instead_of_no_records() {
        let mut menu = ColumnarMenu::default().with_name("testmenu");
        let mut editor = Editor::default();

        // A settled, genuinely empty result shows "NO RECORDS FOUND" on 1 line.
        let mut empty = FakeCompleter::new(&[]);
        setup_menu(&mut menu, &mut editor, &mut empty, (30, 10));
        assert_eq!(menu.get_rows(), 1);
        assert!(menu.menu_string(10, false).contains("NO RECORDS FOUND"));

        // A pending result (background work still in flight) reserves no space and
        // draws nothing, so the menu never flashes "NO RECORDS FOUND".
        let mut pending = PendingCompleter;
        setup_menu(&mut menu, &mut editor, &mut pending, (30, 10));
        assert_eq!(menu.get_rows(), 0);
        assert_eq!(menu.menu_required_lines(30), 0);
        assert!(menu.menu_string(10, false).is_empty());
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
