//! Menu for displaying and applying diagnostic fixes.
//!
//! This menu shows available code fixes for diagnostics at the cursor position,
//! using bordered rendering similar to IdeMenu.

use super::ide_menu::{split_string, truncate_string_list, BorderSymbols};
use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor, lsp::Replacement, painting::Painter, Completer, Suggestion, UndoBehavior,
};
use nu_ansi_term::{ansi::RESET, Style};
use unicode_width::UnicodeWidthStr;

/// A fix option that can be applied to the buffer.
#[derive(Debug, Clone)]
pub struct FixOption {
    /// Title of the fix (shown in the menu)
    pub title: String,
    /// Description of the fix (shown in description panel)
    pub description: Option<String>,
    /// The replacements to apply
    pub replacements: Vec<Replacement>,
}

impl FixOption {
    /// Create a new fix option.
    pub fn new(title: impl Into<String>, replacements: Vec<Replacement>) -> Self {
        Self {
            title: title.into(),
            description: None,
            replacements,
        }
    }

    /// Create a new fix option with a description.
    pub fn with_description(
        title: impl Into<String>,
        description: impl Into<String>,
        replacements: Vec<Replacement>,
    ) -> Self {
        Self {
            title: title.into(),
            description: Some(description.into()),
            replacements,
        }
    }

    /// Create a fix option from a code action.
    pub fn from_code_action(action: &crate::lsp::CodeAction) -> Self {
        Self {
            title: action.title.clone(),
            description: Some(action.fix.description.clone()),
            replacements: action.fix.replacements.clone(),
        }
    }
}

/// Working details calculated during layout
struct WorkingDetails {
    /// Column position of the cursor
    cursor_col: u16,
    /// Calculated width of the completion box
    completion_width: u16,
    /// Space to the left of the menu
    space_left: u16,
    /// Width of the description panel
    description_width: u16,
    /// Whether description is on the right side
    description_is_right: bool,
    /// Offset between completion and description boxes
    description_offset: u16,
}

impl Default for WorkingDetails {
    fn default() -> Self {
        Self {
            cursor_col: 0,
            completion_width: 0,
            space_left: 0,
            description_width: 30,
            description_is_right: true,
            description_offset: 1,
        }
    }
}

/// Menu for displaying and applying diagnostic fixes.
///
/// Uses bordered rendering similar to IdeMenu.
pub struct DiagnosticFixMenu {
    /// Menu settings (name, color, etc.)
    settings: MenuSettings,
    /// Whether the menu is active
    active: bool,
    /// Available fix options
    fixes: Vec<FixOption>,
    /// Selected index
    selected: usize,
    /// Number of values to skip for scrolling
    skip_values: usize,
    /// Border configuration
    border: Option<BorderSymbols>,
    /// Working details calculated during layout
    working_details: WorkingDetails,
    /// Max width of the completion box
    max_completion_width: u16,
    /// Max height of the completion box
    max_completion_height: u16,
    /// Max width of the description panel
    max_description_width: u16,
    /// Longest fix title width (cached)
    longest_title: usize,
}

impl Default for DiagnosticFixMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("diagnostic_fix_menu"),
            active: false,
            fixes: Vec::new(),
            selected: 0,
            skip_values: 0,
            border: Some(BorderSymbols::default()),
            working_details: WorkingDetails::default(),
            max_completion_width: 50,
            max_completion_height: 10,
            max_description_width: 50,
            longest_title: 0,
        }
    }
}

impl MenuBuilder for DiagnosticFixMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

impl DiagnosticFixMenu {
    /// Enable default border
    pub fn with_default_border(mut self) -> Self {
        self.border = Some(BorderSymbols::default());
        self
    }

    /// Update the available fixes.
    pub fn set_fixes(&mut self, fixes: Vec<FixOption>) {
        self.fixes = fixes;
        self.selected = 0;
        self.skip_values = 0;
        self.longest_title = self
            .fixes
            .iter()
            .map(|f| f.title.width())
            .max()
            .unwrap_or(0);
    }

    /// Check if there are any fixes available.
    pub fn has_fixes(&self) -> bool {
        !self.fixes.is_empty()
    }

    /// Get the currently selected fix.
    fn get_selected_fix(&self) -> Option<&FixOption> {
        self.fixes.get(self.selected)
    }

    /// Create a bordered line for a fix option
    fn create_fix_line(&self, fix: &FixOption, index: usize, use_ansi_coloring: bool) -> String {
        let border_width = if self.border.is_some() { 2 } else { 0 };
        let vertical_border = self
            .border
            .as_ref()
            .map(|b| b.vertical)
            .unwrap_or_default();

        let is_selected = index == self.selected;
        let indicator = if is_selected { ">" } else { " " };

        // +2 for indicator and space after it
        let content_width = fix.title.width() + 2;
        let padding_right = (self.working_details.completion_width as usize)
            .saturating_sub(content_width + border_width);

        if use_ansi_coloring {
            let style = if is_selected {
                Style::new().bold().reverse()
            } else {
                Style::new()
            };
            format!(
                "{}{}{}{}{}{}{}",
                vertical_border,
                style.prefix(),
                indicator,
                fix.title,
                " ".repeat(padding_right),
                RESET,
                vertical_border
            )
        } else {
            format!(
                "{}{}{}{}{}",
                vertical_border,
                indicator,
                fix.title,
                " ".repeat(padding_right),
                vertical_border
            )
        }
    }

    /// Create description panel lines
    fn create_description(
        &self,
        description: &str,
        use_ansi_coloring: bool,
        available_height: u16,
    ) -> Vec<String> {
        if description.is_empty() || self.working_details.description_width == 0 {
            return Vec::new();
        }

        let border_width = if self.border.is_some() { 2 } else { 0 };
        let content_width = self
            .working_details
            .description_width
            .saturating_sub(border_width) as usize;
        let content_height = available_height.saturating_sub(border_width) as usize;

        let mut lines = split_string(description, content_width);

        if lines.len() > content_height {
            lines.truncate(content_height);
            truncate_string_list(&mut lines, "...");
        }

        let actual_width = lines
            .iter()
            .map(|s| s.width())
            .max()
            .unwrap_or(0)
            .max(content_width);

        if let Some(border) = &self.border {
            let horizontal_border = border.horizontal.to_string().repeat(actual_width);

            for line in &mut lines {
                let padding = " ".repeat(actual_width.saturating_sub(line.width()));
                if use_ansi_coloring {
                    let style = Style::new().italic();
                    *line = format!(
                        "{}{}{}{}{}{}",
                        border.vertical,
                        style.prefix(),
                        line,
                        padding,
                        RESET,
                        border.vertical
                    );
                } else {
                    *line = format!("{}{}{}{}", border.vertical, line, padding, border.vertical);
                }
            }

            lines.insert(
                0,
                format!(
                    "{}{}{}",
                    border.top_left, horizontal_border, border.top_right
                ),
            );
            lines.push(format!(
                "{}{}{}",
                border.bottom_left, horizontal_border, border.bottom_right
            ));
        }

        lines
    }
}

impl Menu for DiagnosticFixMenu {
    fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn can_quick_complete(&self) -> bool {
        true
    }

    fn can_partially_complete(
        &mut self,
        _values_updated: bool,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
    ) -> bool {
        false
    }

    fn menu_event(&mut self, event: MenuEvent) {
        match event {
            MenuEvent::Activate(_) => {
                self.active = true;
                self.selected = 0;
                self.skip_values = 0;
            }
            MenuEvent::Deactivate => {
                self.active = false;
            }
            MenuEvent::NextElement => {
                if !self.fixes.is_empty() {
                    self.selected = (self.selected + 1) % self.fixes.len();
                    // Handle scrolling
                    let visible_items = self.max_completion_height.saturating_sub(2) as usize;
                    if self.selected >= self.skip_values + visible_items {
                        self.skip_values = self.selected.saturating_sub(visible_items - 1);
                    } else if self.selected < self.skip_values {
                        self.skip_values = self.selected;
                    }
                }
            }
            MenuEvent::PreviousElement => {
                if !self.fixes.is_empty() {
                    self.selected = self
                        .selected
                        .checked_sub(1)
                        .unwrap_or(self.fixes.len() - 1);
                    // Handle scrolling
                    if self.selected < self.skip_values {
                        self.skip_values = self.selected;
                    }
                }
            }
            _ => {}
        }
    }

    fn update_values(&mut self, _editor: &mut Editor, _completer: &mut dyn Completer) {
        // Fixes are set via set_fixes(), nothing to update from completer
    }

    fn update_working_details(
        &mut self,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        let border_width = if self.border.is_some() { 2 } else { 0 };
        let terminal_width = painter.screen_width();

        // Calculate completion box width based on longest title
        let min_width = 15u16;
        let content_width = (self.longest_title + 3) as u16; // +3 for indicator, space, padding
        self.working_details.completion_width = content_width
            .saturating_add(border_width)
            .max(min_width)
            .min(self.max_completion_width)
            .min(terminal_width.saturating_sub(2)); // Don't exceed terminal width

        // Position menu at start of line (small offset for prompt)
        // Diagnostic fix menus should appear at the diagnostic location
        self.working_details.space_left = 0;

        // Calculate space available for description
        let space_after_completion = terminal_width
            .saturating_sub(self.working_details.completion_width)
            .saturating_sub(self.working_details.description_offset);

        if space_after_completion >= 15 {
            self.working_details.description_is_right = true;
            self.working_details.description_width =
                space_after_completion.min(self.max_description_width);
        } else {
            // Not enough space for description
            self.working_details.description_is_right = true;
            self.working_details.description_width = 0;
        }
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        if let Some(fix) = self.get_selected_fix() {
            // Apply all replacements in reverse order to avoid offset issues
            let mut replacements = fix.replacements.clone();
            replacements.sort_by(|a, b| b.span.start.cmp(&a.span.start));

            let mut line_buffer = editor.line_buffer().clone();
            let buffer = line_buffer.get_buffer();

            let mut new_buffer = buffer.to_string();
            for replacement in &replacements {
                let start = replacement.span.start.min(new_buffer.len());
                let end = replacement.span.end.min(new_buffer.len());
                new_buffer.replace_range(start..end, &replacement.new_text);
            }

            // Place cursor at end of first replacement
            let cursor_pos = if let Some(first) = fix.replacements.first() {
                first.span.start + first.new_text.len()
            } else {
                line_buffer.insertion_point()
            };

            line_buffer.set_buffer(new_buffer);
            line_buffer.set_insertion_point(cursor_pos.min(line_buffer.get_buffer().len()));
            editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
        }
    }

    fn min_rows(&self) -> u16 {
        let border_height = if self.border.is_some() { 2 } else { 0 };
        (self.fixes.len() as u16).saturating_add(border_height)
    }

    fn get_values(&self) -> &[Suggestion] {
        // Return empty - we don't use Suggestion directly
        &[]
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        let border_height = if self.border.is_some() { 2 } else { 0 };
        (self.fixes.len() as u16)
            .saturating_add(border_height)
            .min(self.max_completion_height)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        use itertools::EitherOrBoth::{Both, Left, Right};
        use itertools::Itertools;

        if self.fixes.is_empty() {
            return String::from("No fixes available");
        }

        let border_height = if self.border.is_some() { 2 } else { 0 };
        let available_lines = available_lines.min(self.max_completion_height);
        let available_items = available_lines.saturating_sub(border_height) as usize;

        // Create fix lines
        let mut strings: Vec<String> = self
            .fixes
            .iter()
            .skip(self.skip_values)
            .take(available_items)
            .enumerate()
            .map(|(idx, fix)| {
                let actual_idx = idx + self.skip_values;
                self.create_fix_line(fix, actual_idx, use_ansi_coloring)
            })
            .collect();

        // Add borders
        if let Some(border) = &self.border {
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

        // Add description panel if selected fix has one
        let description_lines = self
            .get_selected_fix()
            .and_then(|fix| fix.description.as_ref())
            .map(|desc| self.create_description(desc, use_ansi_coloring, available_lines))
            .unwrap_or_default();

        let distance_left = " ".repeat(self.working_details.space_left as usize);

        // Horizontally join description with fix lines
        if self.working_details.description_is_right {
            for (idx, pair) in strings
                .clone()
                .iter()
                .zip_longest(description_lines.iter())
                .enumerate()
            {
                match pair {
                    Both(_fix_line, desc_line) => {
                        strings[idx] = format!(
                            "{}{}{}{}",
                            distance_left,
                            strings[idx],
                            " ".repeat(self.working_details.description_offset as usize),
                            desc_line,
                        );
                    }
                    Left(fix_line) => {
                        strings[idx] = format!("{}{}", distance_left, fix_line);
                    }
                    Right(desc_line) => {
                        strings.push(format!(
                            "{}{}{}",
                            " ".repeat(
                                self.working_details.space_left as usize
                                    + self.working_details.completion_width as usize
                                    + self.working_details.description_offset as usize
                            ),
                            "",
                            desc_line,
                        ));
                    }
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
                    Both(_fix_line, desc_line) => {
                        strings[idx] = format!(
                            "{}{}{}{}",
                            desc_line,
                            " ".repeat(self.working_details.description_offset as usize),
                            distance_left,
                            strings[idx],
                        );
                    }
                    Left(fix_line) => {
                        let desc_padding = self.working_details.description_width as usize
                            + self.working_details.description_offset as usize;
                        strings[idx] =
                            format!("{}{}{}", " ".repeat(desc_padding), distance_left, fix_line);
                    }
                    Right(desc_line) => {
                        strings.push(desc_line.clone());
                    }
                }
            }
        }

        strings.join("\r\n")
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.working_details.cursor_col = pos.0;
    }
}
