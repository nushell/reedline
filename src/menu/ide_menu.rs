use super::{menu_functions::find_common_string, Menu, MenuEvent, MenuTextStyle};
use crate::{
    core_editor::Editor, menu_functions::string_difference, painting::Painter, Completer,
    Suggestion, UndoBehavior,
};
use nu_ansi_term::{ansi::RESET, Style};

/// Default values used as reference for the menu. These values are set during
/// the initial declaration of the menu and are always kept as reference for the
/// changeable [`IdeMenuDetails`] values.
struct DefaultIdeMenuDetails {
    pub min_width: usize,
    /// padding to the left and right of the suggestions
    pub padding: usize,
    /// Whether the menu has a border or not
    pub border: bool,
    /// horizontal offset from the cursor.
    /// 0 means the top left corner of the menu is below the cursor
    pub cursor_offset: i16,
}

impl Default for DefaultIdeMenuDetails {
    fn default() -> Self {
        Self { 
            min_width: 0,
            padding: 0,
            border: false,
            cursor_offset: 0,
        }
    }
}

#[derive(Default)]
struct IdeMenuDetails {
    // Width of the menu, including the padding and border
    pub width: usize,
    /// Distance from the left side of the terminal to the completion menu
    pub left_distance: usize,
    /// Distance from the right side of the terminal to the completion menu
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

    /// Menu builder with new value for width value
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.default_details.min_width = width;
        self
    }

    /// Menu builder with new value for padding value
    #[must_use]
    pub fn with_padding(mut self, padding: usize) -> Self {
        self.default_details.padding = padding;
        self
    }

    /// Menu builder with new value for border value
    #[must_use]
    pub fn with_border(mut self, border: bool) -> Self {
        self.default_details.border = border;
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

    /// Calculates how many rows the Menu will use
    fn get_rows(&self) -> u16 {
        let values = self.get_values().len() as u16;

        if values == 0 {
            // When the values are empty the no_records_msg is shown, taking 1 line
            return 1;
        }

        if self.default_details.border {
            // top and bottom border take 1 line each
            return values + 2;
        } 

        values
    }

    /// Returns working details width
    fn get_width(&self) -> usize {
        self.working_details.width
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

    fn create_string(
        &self,
        suggestion: &Suggestion,
        index: usize,
        use_ansi_coloring: bool
    ) -> String {
        let vertical_border = if self.default_details.border { "│" } else { "" };

        if use_ansi_coloring {
            let padding_right = self.longest_suggestion.saturating_sub(suggestion.value.len()) + self.default_details.padding;

            if index == self.index() {
                format!(
                    "{}{}{}{}{}{}{}",
                    vertical_border,
                    self.color.selected_text_style.prefix(),
                    " ".repeat(self.default_details.padding),
                    suggestion.value,
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
                    suggestion.value,
                    " ".repeat(padding_right),
                    RESET,
                    vertical_border,
                )
            }

        } else {
            todo!()
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
            let cursor_pos = crossterm::cursor::position().unwrap().0;

            let menu_width = self.longest_suggestion + self.default_details.padding * 2 + if self.default_details.border { 2 } else { 0 };
            self.working_details.width = menu_width.max(self.default_details.min_width);

            let potential_left_distance = cursor_pos as i16 + self.default_details.cursor_offset;

            let left_distance = if potential_left_distance + self.get_width() as i16 > terminal_width as i16 {
                terminal_width.saturating_sub(self.get_width() as u16)
            } else if potential_left_distance < 0 {
                0
            } else {
                potential_left_distance as u16
            };

            let right_distance = (terminal_width as usize).saturating_sub(left_distance as usize + self.get_width());

            self.working_details.left_distance = left_distance as usize;
            self.working_details.right_distance = right_distance;

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
            // The skip values represent the number of lines that should be skipped
            // while printing the menu
            let skip_values = if self.selected >= available_lines {
                let skip_lines = self.selected.saturating_sub(available_lines) + 1;
                skip_lines as usize
            } else {
                0
            };

            let available_values = self.get_values().len();

            let mut strings = self.get_values()
                .iter()
                .skip(skip_values)
                .take(available_values)
                .enumerate()
                .map(|(index, suggestion)| {
                    // Correcting the enumerate index based on the number of skipped values
                    let index = index + skip_values;
                    self.create_string(suggestion, index, use_ansi_coloring)
                })
                .collect::<Vec<String>>();

            if self.default_details.border {
                let inner_width = self.working_details.width.saturating_sub(2);
                strings.insert(0, format!(
                    "╭{}╮",
                    "─".repeat(inner_width)
                ));

                strings.push(format!(
                    "╰{}╯",
                    "─".repeat(inner_width)
                ));
            }

            strings.iter_mut().for_each(|string| {
                string.insert_str(0, &" ".repeat(self.working_details.left_distance));
            });

            strings.join("\r\n")
        }
    }


}