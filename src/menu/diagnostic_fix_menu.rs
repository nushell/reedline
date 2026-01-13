//! Menu for displaying and applying diagnostic fixes.
//!
//! This menu shows available code fixes for diagnostics at the cursor position,
//! similar to the "quick fix" menu in IDEs.

use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor, lsp::Replacement, painting::Painter, Completer, Suggestion, UndoBehavior,
};
use nu_ansi_term::ansi::RESET;

/// A fix option that can be applied to the buffer.
#[derive(Debug, Clone)]
pub struct FixOption {
    /// Title/description of the fix
    pub title: String,
    /// The replacements to apply
    pub replacements: Vec<Replacement>,
}

impl FixOption {
    /// Create a new fix option.
    pub fn new(title: impl Into<String>, replacements: Vec<Replacement>) -> Self {
        Self {
            title: title.into(),
            replacements,
        }
    }

    /// Create a fix option from a code action.
    pub fn from_code_action(action: &crate::lsp::CodeAction) -> Self {
        Self {
            title: action.title.clone(),
            replacements: action.fix.replacements.clone(),
        }
    }
}

/// Menu for displaying and applying diagnostic fixes.
pub struct DiagnosticFixMenu {
    /// Menu settings
    settings: MenuSettings,
    /// Whether menu is active
    active: bool,
    /// Available fix options
    fixes: Vec<FixOption>,
    /// Selected index
    selected: usize,
    /// Current menu event
    event: Option<MenuEvent>,
}

impl Default for DiagnosticFixMenu {
    fn default() -> Self {
        Self {
            settings: MenuSettings::default().with_name("diagnostic_fix_menu"),
            active: false,
            fixes: Vec::new(),
            selected: 0,
            event: None,
        }
    }
}

impl MenuBuilder for DiagnosticFixMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }
}

impl DiagnosticFixMenu {
    /// Update the available fixes.
    pub fn set_fixes(&mut self, fixes: Vec<FixOption>) {
        self.fixes = fixes;
        self.selected = 0;
    }

    /// Check if there are any fixes available.
    pub fn has_fixes(&self) -> bool {
        !self.fixes.is_empty()
    }

    fn move_next(&mut self) {
        if self.selected < self.fixes.len().saturating_sub(1) {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    fn move_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.fixes.len().saturating_sub(1);
        }
    }

    fn get_selected_fix(&self) -> Option<&FixOption> {
        self.fixes.get(self.selected)
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
        match &event {
            MenuEvent::Activate(_) => self.active = true,
            MenuEvent::Deactivate => self.active = false,
            _ => {}
        }
        self.event = Some(event);
    }

    fn update_values(&mut self, _editor: &mut Editor, _completer: &mut dyn Completer) {
        // Fixes are set externally via set_fixes()
    }

    fn update_working_details(
        &mut self,
        _editor: &mut Editor,
        _completer: &mut dyn Completer,
        _painter: &Painter,
    ) {
        if let Some(event) = self.event.take() {
            match event {
                MenuEvent::Activate(_) => {
                    self.selected = 0;
                }
                MenuEvent::NextElement | MenuEvent::MoveDown => self.move_next(),
                MenuEvent::PreviousElement | MenuEvent::MoveUp => self.move_previous(),
                _ => {}
            }
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
        self.fixes.len().max(1) as u16
    }

    fn get_values(&self) -> &[Suggestion] {
        &[]
    }

    fn menu_required_lines(&self, _terminal_columns: u16) -> u16 {
        self.fixes.len().max(1) as u16
    }

    fn menu_string(&self, _available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.fixes.is_empty() {
            return String::from("No fixes available");
        }

        self.fixes
            .iter()
            .enumerate()
            .map(|(index, fix)| {
                let marker = if index == self.selected { "> " } else { "  " };
                let title = &fix.title;

                if use_ansi_coloring {
                    let style = if index == self.selected {
                        &self.settings.color.selected_text_style
                    } else {
                        &self.settings.color.text_style
                    };
                    format!("{}{}{}{}", marker, style.prefix(), title, RESET)
                } else {
                    format!("{}{}", marker, title)
                }
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    fn set_cursor_pos(&mut self, _pos: (u16, u16)) {
        // Not used for simple list menu
    }
}
