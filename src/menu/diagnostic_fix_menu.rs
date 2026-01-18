//! Menu for displaying and applying diagnostic fixes.
//!
//! This menu shows available code fixes for diagnostics at the cursor position,
//! using IdeMenu for rendering with borders and description panels.

use super::{IdeMenu, Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor, lsp::Replacement, painting::Painter, Completer, Span, Suggestion,
    UndoBehavior,
};

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

    /// Convert to a Suggestion for IdeMenu display.
    fn to_suggestion(&self) -> Suggestion {
        Suggestion {
            value: self.title.clone(),
            description: self.description.clone(),
            style: None,
            extra: None,
            span: Span::new(0, 0), // Not used for replacement
            append_whitespace: false,
            match_indices: None,
        }
    }
}

/// Menu for displaying and applying diagnostic fixes.
///
/// Uses IdeMenu internally for consistent visual styling with borders and descriptions.
pub struct DiagnosticFixMenu {
    /// Inner IdeMenu for rendering
    inner: IdeMenu,
    /// Available fix options (parallel to inner menu's suggestions)
    fixes: Vec<FixOption>,
    /// Selected index (tracked separately to find the right fix)
    selected: usize,
}

impl Default for DiagnosticFixMenu {
    fn default() -> Self {
        Self {
            inner: IdeMenu::default()
                .with_name("diagnostic_fix_menu")
                .with_default_border(),
            fixes: Vec::new(),
            selected: 0,
        }
    }
}

impl MenuBuilder for DiagnosticFixMenu {
    fn settings_mut(&mut self) -> &mut MenuSettings {
        self.inner.settings_mut()
    }
}

impl DiagnosticFixMenu {
    /// Update the available fixes and sync with inner IdeMenu.
    pub fn set_fixes(&mut self, fixes: Vec<FixOption>) {
        self.fixes = fixes;
        self.selected = 0;
    }

    /// Check if there are any fixes available.
    pub fn has_fixes(&self) -> bool {
        !self.fixes.is_empty()
    }

    /// Get the currently selected fix.
    fn get_selected_fix(&self) -> Option<&FixOption> {
        self.fixes.get(self.selected)
    }
}

impl Menu for DiagnosticFixMenu {
    fn settings(&self) -> &MenuSettings {
        self.inner.settings()
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
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
        // Track selection changes
        match event {
            MenuEvent::NextElement => {
                if !self.fixes.is_empty() {
                    self.selected = (self.selected + 1) % self.fixes.len();
                }
            }
            MenuEvent::PreviousElement => {
                if !self.fixes.is_empty() {
                    self.selected = self.selected.checked_sub(1).unwrap_or(self.fixes.len() - 1);
                }
            }
            MenuEvent::Activate(_) => {
                self.selected = 0;
            }
            _ => {}
        }
        self.inner.menu_event(event);
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        // Convert fixes to suggestions and update inner menu
        let suggestions: Vec<Suggestion> = self.fixes.iter().map(|f| f.to_suggestion()).collect();

        // Create a temporary completer that returns our suggestions
        struct FixCompleter {
            suggestions: Vec<Suggestion>,
        }
        impl Completer for FixCompleter {
            fn complete(&mut self, _line: &str, _pos: usize) -> Vec<Suggestion> {
                self.suggestions.clone()
            }
        }

        let mut fix_completer = FixCompleter { suggestions };
        self.inner.update_values(editor, &mut fix_completer);

        // Also call original completer update in case it's needed
        let _ = completer;
    }

    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        self.inner.update_working_details(editor, completer, painter);
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
        self.inner.min_rows()
    }

    fn get_values(&self) -> &[Suggestion] {
        self.inner.get_values()
    }

    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.inner.menu_required_lines(terminal_columns)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        if self.fixes.is_empty() {
            return String::from("No fixes available");
        }
        self.inner.menu_string(available_lines, use_ansi_coloring)
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.inner.set_cursor_pos(pos);
    }
}
