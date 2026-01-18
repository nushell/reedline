//! Menu for displaying and applying diagnostic fixes.
//!
//! This menu shows available code fixes for diagnostics at the cursor position,
//! with a simple inline format: replacement text followed by title in parentheses.
//! The menu is positioned below the text being replaced, aligned with the anchor column.

use nu_ansi_term::{ansi::RESET, Style};
use unicode_width::UnicodeWidthStr;

use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor, lsp::Replacement, painting::Painter, Completer, Suggestion, UndoBehavior,
};
// Necessary because of indicator text of two characters `> ` to the left of selected menu item
const LEFT_PADDING: u16 = 2;

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
#[derive(Default)]
struct WorkingDetails {
    /// Space to the left of the menu (includes prompt width + anchor offset)
    space_left: u16,
    /// Cursor column from set_cursor_pos (includes prompt width)
    cursor_col: u16,
}

/// Menu for displaying and applying diagnostic fixes.
///
/// Shows fix options as simple lines: `>replacement_text (title)`
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
    /// Working details calculated during layout
    working_details: WorkingDetails,
    /// Max height of the menu
    max_height: u16,
    /// Anchor column position (start of text being replaced)
    anchor_col: u16,
}

impl Default for DiagnosticFixMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("diagnostic_fix_menu"),
            active: false,
            fixes: Vec::new(),
            selected: 0,
            skip_values: 0,
            working_details: WorkingDetails::default(),
            max_height: 10,
            anchor_col: 0,
        }
    }
}

impl MenuBuilder for DiagnosticFixMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

impl DiagnosticFixMenu {
    /// Update the available fixes with anchor position.
    /// The anchor_col is the column position where the text being replaced starts.
    pub fn set_fixes(&mut self, fixes: Vec<FixOption>, anchor_col: u16) {
        self.fixes = fixes;
        self.selected = 0;
        self.skip_values = 0;
        self.anchor_col = anchor_col;
    }

    /// Check if there are any fixes available.
    pub fn has_fixes(&self) -> bool {
        !self.fixes.is_empty()
    }

    /// Get the currently selected fix.
    fn get_selected_fix(&self) -> Option<&FixOption> {
        self.fixes.get(self.selected)
    }

    /// Format a single fix line: `>replacement_text (title)`
    fn format_fix_line(&self, fix: &FixOption, index: usize, use_ansi_coloring: bool) -> String {
        let is_selected = index == self.selected;
        // let indicator = if is_selected { "" } else { " " };

        // Show the replacement text first, then title in parentheses
        let replacement_text = fix
            .replacements
            .first()
            .map(|r| r.new_text.as_str())
            .unwrap_or("");

        let content = format!(
            "{replacement_text} {}({}){}",
            Style::new().italic().prefix(),
            fix.title,
            RESET
        );

        if is_selected && use_ansi_coloring {
            let style = Style::new().bold().reverse();
            format!("> {}{}{}", style.prefix(), content, RESET)
        } else {
            let style = Style::new().reverse();
            format!("  {}{}{}", style.prefix(), content, RESET)
        }
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
                    let visible_items = self.max_height as usize;
                    if self.selected >= self.skip_values + visible_items {
                        self.skip_values = self.selected.saturating_sub(visible_items - 1);
                    } else if self.selected < self.skip_values {
                        self.skip_values = self.selected;
                    }
                }
            }
            MenuEvent::PreviousElement => {
                if !self.fixes.is_empty() {
                    self.selected = self.selected.checked_sub(1).unwrap_or(self.fixes.len() - 1);
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
        editor: &mut Editor,
        _completer: &mut dyn Completer,
        _painter: &Painter,
    ) {
        // Calculate menu position including prompt width
        // cursor_col = prompt_visual_width + text_before_cursor_visual_width (mod terminal width)
        // We want: prompt_visual_width + anchor_col
        // So: menu_pos = cursor_col - text_before_cursor_visual_width + anchor_col
        let buffer = editor.line_buffer().get_buffer();
        let cursor_pos = editor.line_buffer().insertion_point();
        let text_before_cursor = &buffer[..cursor_pos.min(buffer.len())];
        let cursor_visual_width = text_before_cursor.width() as u16;

        self.working_details.space_left = self
            .working_details
            .cursor_col
            .saturating_sub(cursor_visual_width)
            + self.anchor_col
            - LEFT_PADDING;
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
        self.fixes.len() as u16
    }

    fn get_values(&self) -> &[Suggestion] {
        // Return empty - we don't use Suggestion directly
        &[]
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        (self.fixes.len() as u16).min(self.max_height)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.fixes.is_empty() {
            return String::from("No fixes available");
        }

        let available_lines = available_lines.min(self.max_height) as usize;
        let left_padding = " ".repeat(self.working_details.space_left as usize);

        self.fixes
            .iter()
            .skip(self.skip_values)
            .take(available_lines)
            .enumerate()
            .map(|(idx, fix)| {
                let actual_idx = idx + self.skip_values;
                format!(
                    "{}{}",
                    left_padding,
                    self.format_fix_line(fix, actual_idx, use_ansi_coloring)
                )
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.working_details.cursor_col = pos.0;
    }
}
