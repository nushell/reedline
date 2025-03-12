use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor,
    menu_functions::{can_partially_complete, completer_input, replace_in_buffer},
    painting::Painter,
    Completer, Suggestion,
};
use itertools::{
    EitherOrBoth::{Both, Left, Right},
    Itertools,
};
use nu_ansi_term::ansi::RESET;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The direction of the description box
pub enum DescriptionMode {
    /// Description is always shown on the left
    Left,
    /// Description is always shown on the right
    Right,
    /// Description is shown on the right of the completion if there is enough space
    /// otherwise it is shown on the left
    PreferRight,
}

/// Symbols used for the border of the menu
struct BorderSymbols {
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub horizontal: char,
    pub vertical: char,
}

impl Default for BorderSymbols {
    fn default() -> Self {
        Self {
            top_left: '╭',
            top_right: '╮',
            bottom_left: '╰',
            bottom_right: '╯',
            horizontal: '─',
            vertical: '│',
        }
    }
}

/// Default values used as reference for the menu. These values are set during
/// the initial declaration of the menu and are always kept as reference for the
/// changeable [`IdeMenuDetails`] values.
struct DefaultIdeMenuDetails {
    /// Min width of the completion box, including the border
    pub min_completion_width: u16,
    /// Max width of the completion box, including the border
    pub max_completion_width: u16,
    /// Max height of the completion box, including the border
    /// this will be capped by the lines available in the terminal
    pub max_completion_height: u16,
    /// Padding to the left and right of the suggestions
    pub padding: u16,
    /// Whether the menu has a border or not
    pub border: Option<BorderSymbols>,
    /// Horizontal offset from the cursor.
    /// 0 means the top left corner of the menu is below the cursor
    pub cursor_offset: i16,
    /// How the description is shown
    pub description_mode: DescriptionMode,
    /// Min width of the description, including the border
    /// this will be applied, when the description is "squished"
    /// by the completion box
    pub min_description_width: u16,
    /// Max width of the description, including the border
    pub max_description_width: u16,
    /// Max height of the description, including the border
    pub max_description_height: u16,
    /// Offset from the suggestion box to the description box
    pub description_offset: u16,
    /// If true, the cursor pos will be corrected, so the suggestions match up with the typed text
    /// ```text
    /// C:\> str
    ///      str join
    ///      str trim
    ///      str split
    /// ```
    pub correct_cursor_pos: bool,
}

impl Default for DefaultIdeMenuDetails {
    fn default() -> Self {
        Self {
            min_completion_width: 0,
            max_completion_width: 50,
            max_completion_height: u16::MAX, // will be limited by the available lines
            padding: 0,
            border: None,
            cursor_offset: 0,
            description_mode: DescriptionMode::PreferRight,
            min_description_width: 0,
            max_description_width: 50,
            max_description_height: 10,
            description_offset: 1,
            correct_cursor_pos: false,
        }
    }
}

#[derive(Default)]
struct IdeMenuDetails {
    /// Column of the cursor
    pub cursor_col: u16,
    /// Width of the menu, including the padding and border and the description
    pub menu_width: u16,
    /// width of the completion box, including the padding and border
    pub completion_width: u16,
    /// width of the description box, including the padding and border
    pub description_width: u16,
    /// Where the description box should be shown based on the description mode
    /// and the available space
    pub description_is_right: bool,
    /// Distance from the left side of the terminal to the menu
    pub space_left: u16,
    /// Distance from the right side of the terminal to the menu
    pub space_right: u16,
    /// Corrected description offset, based on the available space
    pub description_offset: u16,
    /// The shortest of the strings, which the suggestions are based on
    pub shortest_base_string: String,
}

/// Menu to present suggestions like similar to Ide completion menus
pub struct IdeMenu {
    /// Menu settings
    settings: MenuSettings,
    /// Ide menu active status
    active: bool,
    /// Default ide menu details that are set when creating the menu
    /// These values are the reference for the working details
    default_details: DefaultIdeMenuDetails,
    /// Working ide menu details keep changing based on the collected values
    working_details: IdeMenuDetails,
    /// Menu cached values
    values: Vec<Suggestion>,
    /// Selected value. Starts at 0
    selected: u16,
    /// Event sent to the menu
    event: Option<MenuEvent>,
    /// Longest suggestion found in the values
    longest_suggestion: usize,
    /// String collected after the menu is activated
    input: Option<String>,
}

impl Default for IdeMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("ide_completion_menu"),
            active: false,
            default_details: DefaultIdeMenuDetails::default(),
            working_details: IdeMenuDetails::default(),
            values: Vec::new(),
            selected: 0,
            event: None,
            longest_suggestion: 0,
            input: None,
        }
    }
}

// Menu configuration functions
impl MenuBuilder for IdeMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

// Menu specific configuration functions
impl IdeMenu {
    /// Menu builder with new value for min completion width
    #[must_use]
    pub fn with_min_completion_width(mut self, width: u16) -> Self {
        self.default_details.min_completion_width = width;
        self
    }

    /// Menu builder with new value for max completion width
    #[must_use]
    pub fn with_max_completion_width(mut self, width: u16) -> Self {
        self.default_details.max_completion_width = width;
        self
    }

    /// Menu builder with new value for max completion height
    #[must_use]
    pub fn with_max_completion_height(mut self, height: u16) -> Self {
        self.default_details.max_completion_height = height;
        self
    }

    /// Menu builder with new value for padding
    #[must_use]
    pub fn with_padding(mut self, padding: u16) -> Self {
        self.default_details.padding = padding;
        self
    }

    /// Menu builder with the default border
    #[must_use]
    pub fn with_default_border(mut self) -> Self {
        self.default_details.border = Some(BorderSymbols::default());
        self
    }

    /// Menu builder with new value for border
    #[must_use]
    pub fn with_border(
        mut self,
        top_right: char,
        top_left: char,
        bottom_right: char,
        bottom_left: char,
        horizontal: char,
        vertical: char,
    ) -> Self {
        self.default_details.border = Some(BorderSymbols {
            top_right,
            top_left,
            bottom_right,
            bottom_left,
            horizontal,
            vertical,
        });
        self
    }

    /// Menu builder with new value for cursor offset
    #[must_use]
    pub fn with_cursor_offset(mut self, cursor_offset: i16) -> Self {
        self.default_details.cursor_offset = cursor_offset;
        self
    }

    /// Menu builder with new description mode
    #[must_use]
    pub fn with_description_mode(mut self, description_mode: DescriptionMode) -> Self {
        self.default_details.description_mode = description_mode;
        self
    }

    /// Menu builder with new min description width
    #[must_use]
    pub fn with_min_description_width(mut self, min_description_width: u16) -> Self {
        self.default_details.min_description_width = min_description_width;
        self
    }

    /// Menu builder with new max description width
    #[must_use]
    pub fn with_max_description_width(mut self, max_description_width: u16) -> Self {
        self.default_details.max_description_width = max_description_width;
        self
    }

    /// Menu builder with new max description height
    #[must_use]
    pub fn with_max_description_height(mut self, max_description_height: u16) -> Self {
        self.default_details.max_description_height = max_description_height;
        self
    }

    /// Menu builder with new description offset
    #[must_use]
    pub fn with_description_offset(mut self, description_offset: u16) -> Self {
        self.default_details.description_offset = description_offset;
        self
    }

    /// Menu builder with new correct cursor pos
    #[must_use]
    pub fn with_correct_cursor_pos(mut self, correct_cursor_pos: bool) -> Self {
        self.default_details.correct_cursor_pos = correct_cursor_pos;
        self
    }
}

// Menu functionality
impl IdeMenu {
    fn move_next(&mut self) {
        if self.selected < (self.values.len() as u16).saturating_sub(1) {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    fn move_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.values.len().saturating_sub(1) as u16;
        }
    }

    fn index(&self) -> usize {
        self.selected as usize
    }

    fn get_value(&self) -> Option<Suggestion> {
        self.values.get(self.index()).cloned()
    }

    /// Calculates how many rows the Menu will try to use (if available)
    fn get_rows(&self) -> u16 {
        let mut values = self.get_values().len() as u16;

        if values == 0 {
            // When the values are empty the no_records_msg is shown, taking 1 line
            return 1;
        }

        if self.default_details.border.is_some() {
            // top and bottom border take 1 line each
            values += 2;
        }

        let description_height = self
            .get_value()
            .and_then(|value| value.description)
            .map(|description| {
                self.description_dims(
                    description,
                    self.working_details.description_width,
                    self.default_details.max_description_height,
                    0,
                )
                .1
            })
            .unwrap_or(0)
            .min(self.default_details.max_description_height);

        values.max(description_height)
    }

    /// Returns working details width
    fn get_width(&self) -> u16 {
        self.working_details.menu_width
    }

    fn reset_position(&mut self) {
        self.selected = 0;
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

    fn create_description(
        &self,
        description: String,
        use_ansi_coloring: bool,
        available_width: u16,
        available_height: u16,
        min_width: u16,
    ) -> Vec<String> {
        if description.is_empty() || available_width == 0 || available_height == 0 {
            return Vec::new();
        }

        let border_width = if self.default_details.border.is_some() {
            2
        } else {
            0
        };

        let content_width = available_width.saturating_sub(border_width);
        let content_height = available_height.saturating_sub(border_width);

        let mut description_lines = split_string(&description, content_width as usize);

        if description_lines.len() > content_height as usize {
            description_lines.truncate(content_height as usize);
            truncate_string_list(&mut description_lines, "...");
        }

        let content_width = description_lines
            .iter()
            .map(|s| s.width())
            .max()
            .unwrap_or_default()
            .max(min_width.saturating_sub(border_width) as usize);

        // let needs_padding = description_lines.len() > 1

        if let Some(border) = &self.default_details.border {
            let horizontal_border = border.horizontal.to_string().repeat(content_width);

            for line in &mut description_lines {
                let padding = " ".repeat(content_width.saturating_sub(line.width()));

                if use_ansi_coloring {
                    *line = format!(
                        "{}{}{}{}{}{}",
                        border.vertical,
                        self.settings.color.description_style.prefix(),
                        line,
                        padding,
                        RESET,
                        border.vertical
                    );
                } else {
                    *line = format!("{}{}{}{}", border.vertical, line, padding, border.vertical);
                }
            }

            description_lines.insert(
                0,
                format!(
                    "{}{}{}",
                    border.top_left, horizontal_border, border.top_right
                ),
            );
            description_lines.push(format!(
                "{}{}{}",
                border.bottom_left, horizontal_border, border.bottom_right
            ));
        } else {
            for line in &mut description_lines {
                let padding = " ".repeat(content_width.saturating_sub(line.width()));

                if use_ansi_coloring {
                    *line = format!(
                        "{}{}{}{}",
                        self.settings.color.description_style.prefix(),
                        line,
                        padding,
                        RESET
                    );
                } else {
                    *line = format!("{}{}", line, padding);
                }
            }
        }

        description_lines
    }

    /// Returns width and height of the description, including the border
    fn description_dims(
        &self,
        description: String,
        max_width: u16,
        max_height: u16,
        min_width: u16,
    ) -> (u16, u16) {
        // we will calculate the uncapped height, the real height
        // will be capped by the available lines

        let lines = self.create_description(description, false, max_width, max_height, min_width);
        let height = lines.len() as u16;
        let string = lines.first().cloned().unwrap_or_default();
        let width = string.width() as u16;
        (width, height)
    }

    fn create_value_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
        use_ansi_coloring: bool,
        padding: usize,
    ) -> String {
        let border_width = if self.default_details.border.is_some() {
            2
        } else {
            0
        };

        let vertical_border = self
            .default_details
            .border
            .as_ref()
            .map(|border| border.vertical)
            .unwrap_or_default();

        let padding_right = (self.working_details.completion_width as usize)
            .saturating_sub(suggestion.value.chars().count() + border_width + padding);

        let max_string_width =
            (self.working_details.completion_width as usize).saturating_sub(border_width + padding);

        let string = if suggestion.value.chars().count() > max_string_width {
            let mut chars = suggestion
                .value
                .chars()
                .take(max_string_width.saturating_sub(3))
                .collect::<String>();
            chars.push_str("...");
            chars
        } else {
            suggestion.value.clone()
        };

        if use_ansi_coloring {
            // strip quotes
            let is_quote = |c: char| "`'\"".contains(c);
            let shortest_base = &self.working_details.shortest_base_string;
            let shortest_base = shortest_base
                .strip_prefix(is_quote)
                .unwrap_or(shortest_base);
            let match_len = shortest_base.len().min(string.len());

            // Split string so the match text can be styled
            let skip_len = string.chars().take_while(|c| is_quote(*c)).count();
            let (match_str, remaining_str) =
                string.split_at((match_len + skip_len).min(string.len()));

            let suggestion_style_prefix = suggestion
                .style
                .unwrap_or(self.settings.color.text_style)
                .prefix();

            if index == self.index() {
                format!(
                    "{}{}{}{}{}{}{}{}{}{}{}{}",
                    vertical_border,
                    suggestion_style_prefix,
                    " ".repeat(padding),
                    self.settings.color.selected_match_style.prefix(),
                    match_str,
                    RESET,
                    suggestion_style_prefix,
                    self.settings.color.selected_text_style.prefix(),
                    remaining_str,
                    " ".repeat(padding_right),
                    RESET,
                    vertical_border,
                )
            } else {
                format!(
                    "{}{}{}{}{}{}{}{}{}{}{}",
                    vertical_border,
                    suggestion_style_prefix,
                    " ".repeat(padding),
                    self.settings.color.match_style.prefix(),
                    match_str,
                    RESET,
                    suggestion_style_prefix,
                    remaining_str,
                    " ".repeat(padding_right),
                    RESET,
                    vertical_border,
                )
            }
        } else {
            let marker = if index == self.index() { ">" } else { "" };

            format!(
                "{}{}{}{}{}{}",
                vertical_border,
                " ".repeat(padding),
                marker,
                string,
                " ".repeat(padding_right),
                vertical_border,
            )
        }
    }
}

impl Menu for IdeMenu {
    /// Menu settings
    fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    /// Deactivates context menu
    fn is_active(&self) -> bool {
        self.active
    }

    /// The ide menu can to quick complete if there is only one element
    fn can_quick_complete(&self) -> bool {
        true
    }

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

    /// Update menu values
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
            .min_by_key(|s| s.len())
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
                MenuEvent::NextElement | MenuEvent::MoveDown => self.move_next(),
                MenuEvent::PreviousElement | MenuEvent::MoveUp => self.move_previous(),
                MenuEvent::MoveLeft
                | MenuEvent::MoveRight
                | MenuEvent::PreviousPage
                | MenuEvent::NextPage => {}
            }

            self.longest_suggestion = self.get_values().iter().fold(0, |prev, suggestion| {
                if prev >= suggestion.value.len() {
                    prev
                } else {
                    suggestion.value.len()
                }
            });

            let terminal_width = painter.screen_width();
            let mut cursor_pos = self.working_details.cursor_col;

            if self.default_details.correct_cursor_pos {
                let base_string = &self.working_details.shortest_base_string;

                cursor_pos = cursor_pos.saturating_sub(base_string.width() as u16);
            }

            let border_width = if self.default_details.border.is_some() {
                2
            } else {
                0
            };

            let description = self
                .get_value()
                .map(|v| {
                    if let Some(v) = v.description {
                        if v.is_empty() {
                            return None;
                        } else {
                            return Some(v);
                        }
                    }
                    None
                })
                .unwrap_or_default();

            let mut min_description_width = if description.is_some() {
                self.default_details.min_description_width
            } else {
                0
            };

            let completion_width = ((self.longest_suggestion.min(u16::MAX as usize) as u16)
                + 2 * self.default_details.padding
                + border_width)
                .min(self.default_details.max_completion_width)
                .max(self.default_details.min_completion_width)
                .min(terminal_width.saturating_sub(min_description_width))
                .max(3 + border_width); // Big enough to show "..."

            let available_description_width = terminal_width
                .saturating_sub(completion_width)
                .min(self.default_details.max_description_width)
                .max(self.default_details.min_description_width)
                .min(terminal_width.saturating_sub(completion_width));

            min_description_width = min_description_width.min(available_description_width);

            let description_width = if let Some(description) = description {
                self.description_dims(
                    description,
                    available_description_width,
                    u16::MAX,
                    min_description_width,
                )
                .0
            } else {
                0
            };

            let max_offset = terminal_width.saturating_sub(completion_width + description_width);

            let description_offset = self.default_details.description_offset.min(max_offset);

            self.working_details.completion_width = completion_width;
            self.working_details.description_width = description_width;
            self.working_details.description_offset = description_offset;
            self.working_details.menu_width =
                completion_width + description_offset + description_width;

            let cursor_offset = self.default_details.cursor_offset;

            self.working_details.description_is_right = match self.default_details.description_mode
            {
                DescriptionMode::Left => false,
                DescriptionMode::Right => true,
                DescriptionMode::PreferRight => {
                    // if there is enough space to the right of the cursor, the description is shown on the right
                    // otherwise it is shown on the left
                    let potential_right_distance = (terminal_width as i16)
                        .saturating_sub(
                            cursor_pos as i16
                                + cursor_offset
                                + description_offset as i16
                                + completion_width as i16,
                        )
                        .max(0) as u16;

                    potential_right_distance >= description_width
                }
            };

            let space_left = (if self.working_details.description_is_right {
                cursor_pos as i16 + cursor_offset
            } else {
                (cursor_pos as i16 + cursor_offset)
                    .saturating_sub(description_width as i16 + description_offset as i16)
            }
            .max(0) as u16)
                .min(terminal_width.saturating_sub(self.get_width()));

            let space_right = terminal_width.saturating_sub(space_left + self.get_width());

            self.working_details.space_left = space_left;
            self.working_details.space_right = space_right;
        }
    }

    /// The buffer gets replaced in the Span location
    fn replace_in_buffer(&self, editor: &mut Editor) {
        replace_in_buffer(self.get_value(), editor);
    }

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16 {
        self.get_rows()
    }

    fn get_values(&self) -> &[Suggestion] {
        &self.values
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        self.get_rows()
            .min(self.default_details.max_completion_height)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.get_values().is_empty() {
            self.no_records_msg(use_ansi_coloring)
        } else {
            let border_width = if self.default_details.border.is_some() {
                2
            } else {
                0
            };

            let available_lines = available_lines.min(self.default_details.max_completion_height);
            // The skip values represent the number of lines that should be skipped
            // while printing the menu
            let skip_values = if self.selected >= available_lines.saturating_sub(border_width) {
                let skip_lines = self
                    .selected
                    .saturating_sub(available_lines.saturating_sub(border_width))
                    + 1;
                skip_lines as usize
            } else {
                0
            };

            let available_values = available_lines.saturating_sub(border_width) as usize;

            let max_padding = self.working_details.completion_width.saturating_sub(
                self.longest_suggestion.min(u16::MAX as usize) as u16 + border_width,
            ) / 2;

            let corrected_padding = self.default_details.padding.min(max_padding) as usize;

            let mut strings = self
                .get_values()
                .iter()
                .skip(skip_values)
                .take(available_values)
                .enumerate()
                .map(|(index, suggestion)| {
                    // Correcting the enumerate index based on the number of skipped values

                    let index = index + skip_values;
                    self.create_value_string(
                        suggestion,
                        index,
                        use_ansi_coloring,
                        corrected_padding,
                    )
                })
                .collect::<Vec<String>>();

            // Add top and bottom border
            if let Some(border) = &self.default_details.border {
                let inner_width = self.working_details.completion_width.saturating_sub(2) as usize;

                strings.insert(
                    0,
                    format!(
                        "{}{}{}",
                        border.top_left,
                        border.horizontal.to_string().repeat(inner_width),
                        border.top_right,
                    ),
                );

                strings.push(format!(
                    "{}{}{}",
                    border.bottom_left,
                    border.horizontal.to_string().repeat(inner_width),
                    border.bottom_right,
                ));
            }

            let description_height =
                available_lines.min(self.default_details.max_description_height);
            let description_lines = self
                .get_value()
                .and_then(|value| value.clone().description)
                .map(|description| {
                    self.create_description(
                        description,
                        use_ansi_coloring,
                        self.working_details.description_width,
                        description_height,
                        self.working_details.description_width, // the width has already been calculated
                    )
                })
                .unwrap_or_default();

            let distance_left = &" ".repeat(self.working_details.space_left as usize);

            // Horizontally join the description lines with the suggestion lines
            if self.working_details.description_is_right {
                for (idx, pair) in strings
                    .clone()
                    .iter()
                    .zip_longest(description_lines.iter())
                    .enumerate()
                {
                    match pair {
                        Both(_suggestion_line, description_line) => {
                            strings[idx] = format!(
                                "{}{}{}{}",
                                distance_left,
                                strings[idx],
                                " ".repeat(self.working_details.description_offset as usize),
                                description_line,
                            )
                        }
                        Left(suggestion_line) => {
                            strings[idx] = format!("{}{}", distance_left, suggestion_line);
                        }
                        Right(description_line) => strings.push(format!(
                            "{}{}",
                            " ".repeat(
                                (self.working_details.completion_width
                                    + self.working_details.description_offset)
                                    as usize
                            ) + distance_left,
                            description_line,
                        )),
                    }
                }
            } else {
                for (idx, pair) in strings
                    .clone()
                    .iter()
                    .zip_longest(description_lines.iter())
                    .enumerate()
                {
                    match pair {
                        Both(suggestion_line, description_line) => {
                            strings[idx] = format!(
                                "{}{}{}{}",
                                distance_left,
                                description_line,
                                " ".repeat(self.working_details.description_offset as usize),
                                suggestion_line,
                            )
                        }
                        Left(suggestion_line) => {
                            strings[idx] = format!(
                                "{}{}",
                                " ".repeat(
                                    (self.working_details.description_width
                                        + self.working_details.description_offset)
                                        as usize
                                ) + distance_left,
                                suggestion_line,
                            );
                        }
                        Right(description_line) => {
                            strings.push(format!("{}{}", distance_left, description_line,))
                        }
                    }
                }
            }

            strings.join("\r\n")
        }
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.working_details.cursor_col = pos.0;
    }
}

/// Split the input into strings that are at most `max_length` (in columns, not in chars) long
/// The split is done at whitespace if possible
fn split_string(input_str: &str, max_length: usize) -> Vec<String> {
    let whitespace_split = input_str.split_whitespace();
    let mut words = Vec::new();

    for word in whitespace_split {
        let word_len_cols = word.width();

        if word_len_cols > max_length {
            let mut width = 0;
            let mut substring = String::new();
            for grapheme in word.graphemes(true) {
                let grapheme_width = grapheme.width();
                // Some unicode characters can have a width of multiple rows
                if grapheme_width > max_length {
                    continue;
                }
                if width + grapheme_width > max_length {
                    words.push(substring);
                    substring = String::from(grapheme);
                    width = grapheme_width;
                } else {
                    substring.push_str(grapheme);
                    width += grapheme_width;
                }
            }
            if !substring.is_empty() {
                words.push(substring);
            }
        } else {
            words.push(word.to_string());
        }
    }

    let mut result = Vec::new();
    let mut string = String::new();

    for word in words {
        if string.width() + word.width() > max_length {
            result.push(string.trim_end().to_string());
            string = word;
            string.push(' ');
        } else {
            string.push_str(&word);
            string.push(' ');
        }
    }

    if !string.trim_end().is_empty() {
        result.push(string.trim_end().to_string());
    }

    result
}

/// Truncate a list of strings using the provided truncation characters
fn truncate_string_list(list: &mut [String], truncation_chars: &str) {
    let truncation_chars: Vec<char> = truncation_chars.chars().rev().collect();
    let truncation_len = truncation_chars.len();
    let mut to_replace = truncation_len;

    'outer: for line in list.iter_mut().rev() {
        let chars = UnicodeSegmentation::graphemes(line.as_str(), true).collect::<Vec<&str>>();
        let mut new_line = String::new();
        for grapheme in chars.into_iter().rev() {
            if to_replace > 0 {
                new_line.insert(0, truncation_chars[truncation_len - to_replace]);
                to_replace -= 1;
            } else {
                new_line.insert_str(0, grapheme);
            }
        }
        *line = new_line;
        if to_replace == 0 {
            break 'outer;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Span, UndoBehavior};

    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case(
        "",
        10,
        vec![]
    )]
    #[case(
        "description",
        15,
        vec![
            "description".into(),
        ]
    )]
    #[case(
        "this is a description",
        10,
        vec![
            "this is a".into(),
            "descriptio".into(),
            "n".into(),
        ]
    )]
    #[case(
        "this is another description",
        2,
        vec![
            "th".into(),
            "is".into(),
            "is".into(),
            "an".into(),
            "ot".into(),
            "he".into(),
            "r".into(),
            "de".into(),
            "sc".into(),
            "ri".into(),
            "pt".into(),
            "io".into(),
            "n".into(),
        ]
    )]
    #[case(
        "this is a description",
        10,
        vec![
            "this is a".into(),
            "descriptio".into(),
            "n".into(),
        ]
    )]
    #[case(
        "this is a description",
        10,
        vec![
            "this is a".into(),
            "descriptio".into(),
            "n".into(),
        ]
    )]
    #[case(
        "this is a description",
        12,
        vec![
            "this is a".into(),
            "description".into(),
        ]
    )]
    #[case(
        "test",
        1,
        vec![
            "t".into(),
            "e".into(),
            "s".into(),
            "t".into(),
        ]
    )]
    #[case(
        "😊a😊 😊bc de😊fg",
        2,
        vec![
            "😊".into(),
            "a".into(),
            "😊".into(),
            "😊".into(),
            "bc".into(),
            "de".into(),
            "😊".into(),
            "fg".into(),
        ]
    )]
    #[case(
        "😊",
        1,
        vec![],
    )]
    #[case(
        "t😊e😊s😊t",
        1,
        vec![
            "t".into(),
            "e".into(),
            "s".into(),
            "t".into(),
        ]
    )]

    fn test_split_string(
        #[case] input: &str,
        #[case] max_width: usize,
        #[case] expected: Vec<String>,
    ) {
        let result = split_string(input, max_width);

        assert_eq!(result, expected)
    }

    #[rstest]
    #[case(
        &mut vec![
            "this is a description".into(),
            "that will be truncate".into(),
            "d".into(),
        ],
        "...",
        vec![
            "this is a description".into(),
            "that will be trunca..".into(),
            ".".into(),
        ]
    )]
    #[case(
        &mut vec![
            "this is a description".into(),
            "that will be truncate".into(),
            "d".into(),
        ],
        "....",
        vec![
            "this is a description".into(),
            "that will be trunc...".into(),
            ".".into(),
        ]
    )]
    #[case(
        &mut vec![
            "😊a😊 😊bc de😊fg".into(),
            "😊a😊 😊bc de😊fg".into(),
            "😊a😊 😊bc de😊fg".into(),
        ],
        "...",
        vec![
            "😊a😊 😊bc de😊fg".into(),
            "😊a😊 😊bc de😊fg".into(),
            "😊a😊 😊bc de...".into(),
        ]
    )]
    #[case(
        &mut vec![
            "t".into(),
            "e".into(),
            "s".into(),
            "t".into(),
        ],
        "..",
        vec![
            "t".into(),
            "e".into(),
            ".".into(),
            ".".into(),
        ]
    )]
    #[case(
        &mut vec![
            "😊".into(),
            "😊".into(),
            "s".into(),
            "t".into(),
        ],
        "..😊",
        vec![
            "😊".into(),
            ".".into(),
            ".".into(),
            "😊".into(),
        ]
    )]
    #[case(
        &mut vec![
            "".into(),
        ],
        "test",
        vec![
            "".into()
        ],
    )]
    #[case(
        &mut vec![
            "t".into(),
            "e".into(),
            "s".into(),
            "t".into()
        ],
        "",
        vec![
            "t".into(),
            "e".into(),
            "s".into(),
            "t".into()
        ],
    )]

    fn test_truncate_list_string(
        #[case] input: &mut Vec<String>,
        #[case] truncation_chars: &str,
        #[case] expected: Vec<String>,
    ) {
        truncate_string_list(input, truncation_chars);

        assert_eq!(*input, expected)
    }

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
        let mut menu = IdeMenu::default().with_name("testmenu");
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
    fn test_regression_panic_on_long_item() {
        let commands = vec![
            "hello world 2".into(),
            "hello another very large option for hello word that will force one column".into(),
            "this is the reedline crate".into(),
            "abaaabas".into(),
            "abaaacas".into(),
        ];

        let mut completer = Box::new(crate::DefaultCompleter::new_with_wordlen(commands, 2));

        let mut menu = IdeMenu::default().with_name("testmenu");
        menu.working_details = IdeMenuDetails {
            cursor_col: 50,
            menu_width: 50,
            completion_width: 50,
            description_width: 50,
            description_is_right: true,
            space_left: 50,
            space_right: 50,
            description_offset: 50,
            shortest_base_string: String::new(),
        };
        let mut editor = Editor::default();
        // backtick at the end of the line
        editor.set_buffer(
            "hello another very large option for hello word that will force one colu".to_string(),
            UndoBehavior::CreateUndoPoint,
        );

        menu.update_values(&mut editor, &mut *completer);

        menu.menu_string(500, true);
    }

    #[test]
    fn test_menu_create_value_string() {
        // https://github.com/nushell/nushell/issues/13951
        let mut completer = FakeCompleter::new(&["おはよう", "`おはよう(`"]);
        let mut menu = IdeMenu::default().with_name("testmenu");
        menu.working_details = IdeMenuDetails {
            cursor_col: 50,
            menu_width: 50,
            completion_width: 50,
            description_width: 50,
            description_is_right: true,
            space_left: 50,
            space_right: 50,
            description_offset: 50,
            shortest_base_string: String::new(),
        };
        let mut editor = Editor::default();

        editor.set_buffer("おは".to_string(), UndoBehavior::CreateUndoPoint);
        menu.update_values(&mut editor, &mut completer);
        assert!(menu.menu_string(2, true).contains("`おは"));
    }
}
