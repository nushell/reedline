use super::{menu_functions::find_common_string, Menu, MenuEvent, MenuTextStyle};
use crate::{
    core_editor::Editor, menu_functions::string_difference, painting::{Painter, strip_ansi}, Completer,
    Suggestion, UndoBehavior,
};
use nu_ansi_term::{ansi::RESET, Style};
use itertools::{EitherOrBoth::{Both, Left, Right}, Itertools};

pub enum DescriptionMode {
    /// Description is shown on the right of the completion if there is enough space
    /// otherwise it is shown on the left
    PreferRight,
    /// Description is always shown on the right
    Right,
    /// Description is always shown on the left
    Left,
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
    /// Minimum width of the completion box, 
    pub min_completion_width: usize,
    /// max height of the completion box, including the border
    /// this will be capped by the lines available in the terminal
    pub max_completion_height: u16,
    /// Padding to the left and right of the suggestions
    pub padding: usize,
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
    pub min_description_width: usize,
    /// Max width of the description, including the border
    pub max_description_width: usize,
    /// Max height of the description, including the border
    pub max_description_height: usize,
    /// Offset from the suggestion box to the description box
    pub description_offset: usize,
}

impl Default for DefaultIdeMenuDetails {
    fn default() -> Self {
        Self { 
            min_completion_width: 0,
            max_completion_height: u16::MAX,
            padding: 0,
            border: None,
            cursor_offset: 0,       
            description_mode: DescriptionMode::PreferRight,
            min_description_width: 5,
            max_description_width: 40,
            max_description_height: 10,
            description_offset: 1,
        }
    }
}

#[derive(Default)]
struct IdeMenuDetails {
    /// current width of the terminal
    pub terminal_width: usize,
    /// Width of the menu, including the padding and border and the description
    pub menu_width: usize,
    /// width of the completion box, including the padding and border
    pub completion_width: usize,
    /// width of the description box, including the padding and border
    pub description_width: usize,
    /// Where the description box should be shown based on the description mode
    /// and the available space
    pub description_is_right: bool,
    /// Distance from the left side of the terminal to the menu
    pub left_distance: usize,
    /// Distance from the right side of the terminal to the menu
    pub right_distance: usize,
}

/// Menu to present suggestions like similar to Ide completion menus
pub struct IdeMenu {
    /// Menu name
    name: String,
    /// Ide menu active status
    active: bool,
    /// Menu coloring
    color: MenuTextStyle,
    /// Default ide menu details that are set when creating the menu
    /// These values are the reference for the working details
    default_details: DefaultIdeMenuDetails,
    /// Working ide menu details keep changing based on the collected values
    working_details: IdeMenuDetails,
    /// Menu cached values
    values: Vec<Suggestion>,
    /// Selected value. Starts at 0
    selected: u16,
    /// Menu marker when active
    marker: String,
    /// Event sent to the menu
    event: Option<MenuEvent>,
    /// Longest suggestion found in the values
    longest_suggestion: usize,
    /// String collected after the menu is activated
    input: Option<String>,
    /// Calls the completer using only the line buffer difference difference
    /// after the menu was activated
    only_buffer_difference: bool,
}

impl Default for IdeMenu {
    fn default() -> Self {
        Self {
            name: "ide_completion_menu".to_string(),
            active: false,
            color: MenuTextStyle::default(),
            default_details: DefaultIdeMenuDetails::default(),
            working_details: IdeMenuDetails::default(),
            values: Vec::new(),
            selected: 0,
            marker: "| ".to_string(),
            event: None,
            longest_suggestion: 0,
            input: None,
            only_buffer_difference: false,
        }
    }
}

impl IdeMenu {
    /// Menu builder with new name
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.into();
        self
    }

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

    /// Menu builder with new value for text style
    #[must_use]
    pub fn with_description_text_style(mut self, description_text_style: Style) -> Self {
        self.color.description_style = description_text_style;
        self
    }

    /// Menu builder with new value for min completion width value
    #[must_use]
    pub fn with_min_completion_width(mut self, width: usize) -> Self {
        self.default_details.min_completion_width = width;
        self
    }

    /// Menu builder with new value for max completion height value
    #[must_use]
    pub fn with_max_completion_height(mut self, height: u16) -> Self {
        self.default_details.max_completion_height = height;
        self
    }

    /// Menu builder with new value for padding value
    #[must_use]
    pub fn with_padding(mut self, padding: usize) -> Self {
        self.default_details.padding = padding;
        self
    }

    /// Menu builder with the default border value
    #[must_use]
    pub fn with_default_border(mut self) -> Self {
        self.default_details.border = Some(BorderSymbols::default());
        self
    }

    /// Menu builder with new value for border value
    #[must_use]
    pub fn with_border(
        mut self, 
        top_right: char, 
        top_left: char, 
        bottom_right: char, 
        bottom_left: char, 
        horizontal: char, 
        vertical: char
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

    /// Menu builder with new value for cursor offset value
    #[must_use]
    pub fn with_cursor_offset(mut self, cursor_offset: i16) -> Self {
        self.default_details.cursor_offset = cursor_offset;
        self
    }

    /// Menu builder with marker
    #[must_use]
    pub fn with_marker(mut self, marker: String) -> Self {
        self.marker = marker;
        self
    }

    /// Menu builder with new only buffer difference
    #[must_use]
    pub fn with_only_buffer_difference(mut self, only_buffer_difference: bool) -> Self {
        self.only_buffer_difference = only_buffer_difference;
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
    pub fn with_min_description_width(mut self, min_description_width: usize) -> Self {
        self.default_details.min_description_width = min_description_width;
        self
    }

    /// Menu builder with new max description width
    #[must_use]
    pub fn with_max_description_width(mut self, max_description_width: usize) -> Self {
        self.default_details.max_description_width = max_description_width;
        self
    }

    /// Menu builder with new max description height
    #[must_use]
    pub fn with_max_description_height(mut self, max_description_height: usize) -> Self {
        self.default_details.max_description_height = max_description_height;
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

        let descripion_height = self.get_value()
            .and_then(|value| value.description)
            .map(|description| self.description_dims(description).1)
            .unwrap_or(0);

        values.max(descripion_height)
    }

    /// Returns working details width
    fn get_width(&self) -> usize {
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
                self.color.selected_text_style.prefix(),
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
        available_lines: u16,
    ) -> Vec<String> {
        let border_width = if self.default_details.border.is_some() { 
            2 
        } else { 
            0 
        };

        let available_width = self.working_details.terminal_width
            .saturating_sub(self.working_details.completion_width + self.default_details.description_offset + border_width);

        let max_width = self.default_details.max_description_width.min(available_width).max(self.default_details.min_description_width);
        let max_height = self.default_details.max_description_height.min(available_lines as usize);

        let content_width = max_width.saturating_sub(border_width);
        let content_height = max_height.saturating_sub(border_width);
        let mut description_lines = split_string(&description, content_width, content_height, "...");

        let needs_padding = description_lines.len() > 1;

        if let Some(border) = &self.default_details.border {
            let horizontal_border = border.horizontal.to_string()
                .repeat(if needs_padding { 
                    content_width 
                } else { 
                    description_lines[0].chars().count()
                });


            for line in &mut description_lines {
                let padding = if needs_padding {
                    " ".repeat(content_width.saturating_sub(line.chars().count()))
                } else {
                    String::new()
                };

                if use_ansi_coloring {
                    *line = format!("{}{}{}{}{}{}", border.vertical, self.color.description_style.prefix(), line, padding, RESET, border.vertical);
                } else {
                    *line = format!("{}{}{}{}", border.vertical, line, padding, border.vertical);
                }
            }

            description_lines.insert(0, format!("{}{}{}", border.top_left, horizontal_border, border.top_right));
            description_lines.push(format!("{}{}{}", border.bottom_left, horizontal_border, border.bottom_right));
        } else {
            for line in &mut description_lines {
                let padding = if needs_padding {
                    " ".repeat(content_width.saturating_sub(line.chars().count()))
                } else {
                    String::new()
                };

                if use_ansi_coloring {
                    *line = format!("{}{}{}{}", self.color.description_style.prefix(), line, padding, RESET);
                } else {
                    *line = format!("{}{}", line, padding);
                }
            }
        }

        description_lines
    }

    /// Returns width and height of the description, including the border
    fn description_dims(&self, description: String) -> (u16, u16) {
        // we will calculate the uncapped height, the real height
        // will be capped by the available lines
        let lines = self.create_description(description, false, u16::MAX);
        let height = lines.len() as u16;
        let string = lines.first().cloned().unwrap_or_default();
        let width = strip_ansi(&string).chars().count() as u16;

        (width, height)
    }

    fn create_value_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
        use_ansi_coloring: bool
    ) -> String {
        let border_width = if self.default_details.border.is_some() { 
            2 
        } else { 
            0 
        };
        let vertical_border = self.default_details.border.as_ref().map(|border| border.vertical).unwrap_or_default();
        let padding_right = self.working_details.completion_width.saturating_sub(suggestion.value.chars().count()).saturating_sub(border_width);

        let max_string_width = self.working_details.completion_width.saturating_sub(border_width);
        
        let string = if suggestion.value.chars().count() > max_string_width {
            let mut chars = suggestion.value.chars().take(max_string_width.saturating_sub(3)).collect::<String>();
            chars.push_str("...");
            chars
        } else {
            suggestion.value.clone()
        };        

        if use_ansi_coloring {
            if index == self.index() {
                format!(
                    "{}{}{}{}{}{}{}",
                    vertical_border,
                    self.color.selected_text_style.prefix(),
                    " ".repeat(self.default_details.padding),
                    string,
                    " ".repeat(padding_right),
                    RESET,
                    vertical_border,
                )
                            
            } else {
                format!(
                    "{}{}{}{}{}{}{}",
                    vertical_border,
                    self.color.text_style.prefix(),
                    " ".repeat(self.default_details.padding),
                    string,
                    " ".repeat(padding_right),
                    RESET,
                    vertical_border,
                )
            }
        } else {
            let marker = if index == self.index() { ">" } else { "" };

            format!(
                "{}{}{}{}{}",
                vertical_border,
                marker,
                string,
                " ".repeat(padding_right),
                vertical_border,
            )
        }
    }
}

impl Menu for IdeMenu {
    /// Menu name
    fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Menu indicator
    fn indicator(&self) -> &str {
        self.marker.as_str()
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

        let values = self.get_values();
        if let (Some(Suggestion { value, span, .. }), Some(index)) = find_common_string(values) {
            let index = index.min(value.len());
            let matching = &value[0..index];

            // make sure that the partial completion does not overwrite user entered input
            let extends_input = matching.starts_with(&editor.get_buffer()[span.start..span.end]);

            if !matching.is_empty() && extends_input {
                let mut line_buffer = editor.line_buffer().clone();
                line_buffer.replace_range(span.start..span.end, matching);

                let offset = if matching.len() < (span.end - span.start) {
                    line_buffer
                        .insertion_point()
                        .saturating_sub((span.end - span.start) - matching.len())
                } else {
                    line_buffer.insertion_point() + matching.len() - (span.end - span.start)
                };

                line_buffer.set_insertion_point(offset);
                editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);

                // The values need to be updated because the spans need to be
                // recalculated for accurate replacement in the string
                self.update_values(editor, completer);

                true
            } else {
                false
            }
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
        if self.only_buffer_difference {
            if let Some(old_string) = &self.input {
                let (start, input) = string_difference(editor.get_buffer(), old_string);
                if !input.is_empty() {
                    self.values = completer.complete(input, start);
                    self.reset_position();
                }
            }
        } else {
            // If there is a new line character in the line buffer, the completer
            // doesn't calculate the suggested values correctly. This happens when
            // editing a multiline buffer.
            // Also, by replacing the new line character with a space, the insert
            // position is maintain in the line buffer.
            let trimmed_buffer = editor.get_buffer().replace('\n', " ");
            self.values = completer.complete(trimmed_buffer.as_str(), editor.insertion_point());
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
            // The working value for the menu are updated first before executing any of the
            match event {
                MenuEvent::Activate(updated) => {
                    self.active = true;
                    self.reset_position();

                    self.input = if self.only_buffer_difference {
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
                MenuEvent::MoveLeft | MenuEvent::MoveRight | MenuEvent::PreviousPage | MenuEvent::NextPage => {}
            }

            self.longest_suggestion = self.get_values().iter().fold(0, |prev, suggestion| {
                if prev >= suggestion.value.len() {
                    prev
                } else {
                    suggestion.value.len()
                }
            });


            let terminal_width = painter.screen_width();
            
            self.working_details.terminal_width = terminal_width as usize;
            let cursor_pos = crossterm::cursor::position().unwrap().0;
            
            let border_width = if self.default_details.border.is_some() { 
                2 
            } else { 
                0 
            };
            // we first estimate the completion, so we can use it to calculate the space for the description
            self.working_details.completion_width = (self.longest_suggestion 
                + self.default_details.padding 
                * 2 
                + border_width)
                .max(self.default_details.min_completion_width);

            self.working_details.description_width = self.get_value()
                .and_then(|value| value.description)
                .map(|description| self.description_dims(description).0)
                .unwrap_or(0) as usize;
            // then cap the completion width to the available space
            let max_completion_width = (terminal_width as usize)
                .saturating_sub(
                    self.default_details.padding 
                    * 2 
                    + border_width 
                    + self.working_details.description_width 
                    + if self.working_details.description_width > 0 { 
                        self.default_details.description_offset 
                    } else { 
                        0 
                    }
                );

            self.working_details.completion_width = self.working_details.completion_width.min(max_completion_width);
            

            self.working_details.menu_width = self.working_details.completion_width + self.working_details.description_width + if self.working_details.description_width > 0 { 
                    self.default_details.description_offset 
                } else { 
                    0 
                };
    
            self.working_details.description_is_right = match self.default_details.description_mode {
                DescriptionMode::Left => false,
                DescriptionMode::PreferRight => {
                    // if there is enough space to the right of the cursor, the description is shown on the right
                    // otherwise it is shown on the left
                    let potential_right_distance = (terminal_width as i16).saturating_sub(cursor_pos as i16 + self.default_details.cursor_offset + self.default_details.description_offset as i16 + self.working_details.completion_width as i16).max(0) as usize	;

                    potential_right_distance >= self.working_details.description_width + self.default_details.description_offset     
                },
                DescriptionMode::Right => true,
            };

            if self.working_details.description_is_right {
                let potential_left_distance = cursor_pos as i16 + self.default_details.cursor_offset;
                let left_distance = potential_left_distance.clamp(0, terminal_width.saturating_sub(self.get_width() as u16) as i16);

                let right_distance = (terminal_width as usize).saturating_sub(left_distance as usize + self.get_width());
                self.working_details.left_distance = left_distance as usize;
                self.working_details.right_distance = right_distance;
            } else {
                let potential_left_distance = cursor_pos as i16 + self.default_details.cursor_offset - self.working_details.description_width as i16 - self.default_details.description_offset as i16;
                let left_distance = potential_left_distance.clamp(0, terminal_width.saturating_sub(self.get_width() as u16) as i16);

                let right_distance = (terminal_width as usize).saturating_sub(left_distance as usize + self.get_width());
                self.working_details.left_distance = left_distance as usize;
                self.working_details.right_distance = right_distance;
            }
        }
    }

    /// The buffer gets replaced in the Span location
    fn replace_in_buffer(&self, editor: &mut Editor) {
        if let Some(Suggestion {
            mut value,
            span,
            append_whitespace,
            ..
        }) = self.get_value()
        {
            let start = span.start.min(editor.line_buffer().len());
            let end = span.end.min(editor.line_buffer().len());
            if append_whitespace {
                value.push(' ');
            }
            let mut line_buffer = editor.line_buffer().clone();
            line_buffer.replace_range(start..end, &value);

            let mut offset = line_buffer.insertion_point();
            offset = offset.saturating_add(value.len());
            offset = offset.saturating_sub(end.saturating_sub(start));
            line_buffer.set_insertion_point(offset);
            editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
        }
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
                let skip_lines = self.selected.saturating_sub(available_lines.saturating_sub(border_width)) + 1;
                skip_lines as usize
            } else {
                0
            };

            let available_values = available_lines.saturating_sub(border_width) as usize;

            let mut strings = self.get_values()
                .iter()
                .skip(skip_values)
                .take(available_values)
                .enumerate()
                .map(|(index, suggestion)| {
                    // Correcting the enumerate index based on the number of skipped values

                    let index = index + skip_values;
                    self.create_value_string(suggestion, index, use_ansi_coloring)
                })
                .collect::<Vec<String>>();

            if let Some(border) = &self.default_details.border {
                let inner_width = self.working_details.completion_width.saturating_sub(2);
                strings.insert(0, format!(
                    "{}{}{}",
                    border.top_left,
                    border.horizontal.to_string().repeat(inner_width),
                    border.top_right,
                ));

                strings.push(format!(
                    "{}{}{}",
                    border.bottom_left,
                    border.horizontal.to_string().repeat(inner_width),
                    border.bottom_right,
                ));
            }

            let description_lines = self.get_value()
                .and_then(|value| value.clone().description)
                .map(|description| self.create_description(description, use_ansi_coloring, available_lines))
                .unwrap_or_default();

            let padding_left = &" ".repeat(self.working_details.left_distance);

            // horizontally join the description lines with the suggestion lines
            if self.working_details.description_is_right {
                for (idx, pair) in strings.clone().iter().zip_longest(description_lines.iter()).enumerate() {
                    match pair {
                        Both(_suggestion_line, description_line) => {
                            strings[idx] = format!(
                                "{}{}{}{}",
                                padding_left,
                                strings[idx],
                                " ".repeat(self.default_details.description_offset),
                                description_line,
                            )
                        },
                        Left(suggestion_line) => {
                            strings[idx] = format!(
                                "{}{}",
                                padding_left,
                                suggestion_line,
                            )
                        },
                        Right(description_line) => {
                            strings.push(format!(
                                "{}{}",
                                " ".repeat(self.working_details.completion_width + self.default_details.description_offset) + padding_left,
                                description_line,
                            ))
                        }
                    }
                }
            } else {
                for (idx, pair) in strings.clone().iter().zip_longest(description_lines.iter()).enumerate() {
                    match pair {
                        Both(suggestion_line, description_line) => {
                            strings[idx] = format!(
                                "{}{}{}{}",
                                padding_left,
                                description_line,
                                " ".repeat(self.default_details.description_offset),
                                suggestion_line,
                            )
                        },
                        Left(suggestion_line) => {
                            strings[idx] = format!(
                                "{}{}",
                                " ".repeat(self.working_details.description_width + self.default_details.description_offset) + padding_left,
                                suggestion_line,
                            )
                        },
                        Right(description_line) => {
                            strings.push(format!(
                                "{}{}",
                                padding_left,
                                description_line,
                            ))
                        }
                    }
                }
            }

            strings.join("\r\n")
        }
    }
}

/// Split the input into strings that are at most `max_width` long
/// The split is done at spaces if possible
fn split_string(input: &str, max_width: usize, max_height: usize, truncation_symbol: &str) -> Vec<String> {
    if max_width == 0 || max_height == 0 {
        return Vec::new();
    }

    let words: Vec<&str> = input.split_whitespace().collect();
    let mut result = Vec::new();
    let mut current_line = String::new();

    for word in words {
        if word.len() > max_width {
            let chars: Vec<char> = word.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let end = usize::min(i + max_width, chars.len());
                result.push(chars[i..end].iter().collect());
                i = end;
            }
        } else if current_line.len() + word.len() + 1 > max_width {
            if !current_line.is_empty() {
                result.push(current_line.trim_end().to_string());
            }
            current_line = String::from(word);
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }

    if !current_line.is_empty() {
        result.push(current_line.trim_end().to_string());
    }
    
    // add the truncation symbol to the last truncation_symbol len characters, not just to the last line
    // this is needed, so we still fit in max_width, even if truncation symbol is larger
    if result.len() > max_height {
        result.truncate(max_height);
        let truncation_len = truncation_symbol.chars().count();
        
        let mut indexes_to_replace: Vec<(usize, usize)> = Vec::new();    
        let mut char_count = 0;
    
        'outer: for (idx, line) in result.iter().enumerate().rev() {
            let chars: Vec<_> = line.chars().collect();
            for (char_idx, _char) in chars.iter().enumerate().rev() {
                indexes_to_replace.push((idx, char_idx));
                char_count += 1;
                if char_count == truncation_len {
                    break 'outer;
                }
            }
        }

        for (idx, char_idx) in indexes_to_replace {
            let mut chars: Vec<_> = result[idx].chars().collect();
            chars[char_idx] = truncation_symbol.chars().next().unwrap();
            result[idx] = chars.iter().collect();
        }
    }

    result
}
