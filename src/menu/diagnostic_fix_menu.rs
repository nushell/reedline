//! Menu for displaying and applying diagnostic fixes.
//!
//! This menu shows available code fixes for diagnostics at the cursor position,
//! with a simple inline format: replacement text followed by title in parentheses.
//! The menu is positioned below the text being replaced, aligned with the anchor column.

use itertools::Itertools;
use lsp_types::{CodeAction, TextEdit};
use nu_ansi_term::{ansi::RESET, Style};
use unicode_width::UnicodeWidthStr;

use super::{Menu, MenuBuilder, MenuEvent, MenuSettings};
use crate::{
    core_editor::Editor,
    lsp::{range_to_span, Span},
    painting::Painter,
    Completer, Suggestion, UndoBehavior,
};

// Necessary because of indicator text of two characters `> ` to the left of selected menu item
const LEFT_PADDING: u16 = 2;

/// Pre-computed fix with byte offsets for buffer manipulation.
#[derive(Debug, Clone)]
struct FixInfo {
    /// Title of the fix (shown in the menu)
    title: String,
    /// The text edits to apply, converted to byte spans
    edits: Vec<(Span, String)>,
}

impl FixInfo {
    /// Get the first replacement text for display
    fn first_replacement_text(&self) -> &str {
        self.edits.first().map_or("", |(_, text)| text.as_str())
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
    /// Available fixes with pre-computed byte offsets
    fixes: Vec<FixInfo>,
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
    /// Update the available fixes from LSP code actions.
    ///
    /// Converts LSP ranges to byte offsets using the provided content.
    pub fn set_fixes(&mut self, actions: Vec<CodeAction>, content: &str, anchor_col: u16) {
        self.fixes = actions
            .into_iter()
            .filter_map(|action| {
                let edits = extract_text_edits(&action)?;
                let edits: Vec<(Span, String)> = edits
                    .into_iter()
                    .map(|edit| (range_to_span(content, &edit.range), edit.new_text))
                    .collect();

                (!edits.is_empty()).then_some(FixInfo {
                    title: action.title,
                    edits,
                })
            })
            .collect();

        self.selected = 0;
        self.skip_values = 0;
        self.anchor_col = anchor_col;
    }

    /// Check if there are any fixes available.
    pub fn has_fixes(&self) -> bool {
        !self.fixes.is_empty()
    }

    /// Get the currently selected fix.
    fn get_selected_fix(&self) -> Option<&FixInfo> {
        self.fixes.get(self.selected)
    }

    /// Format a single fix line: `>replacement_text (title)`
    fn format_fix_line(&self, fix: &FixInfo, index: usize, use_ansi_coloring: bool) -> String {
        let replacement_text = fix.first_replacement_text();

        let content = format!(
            "{replacement_text} {}({}){}",
            Style::new().italic().prefix(),
            fix.title,
            RESET
        );

        let (indicator, style) = match (index == self.selected, use_ansi_coloring) {
            (true, true) => ("> ", Style::new().bold().reverse()),
            _ => ("  ", Style::new().reverse()),
        };

        format!("{indicator}{}{content}{RESET}", style.prefix())
    }

    /// Move selection forward, wrapping around
    fn select_next(&mut self) {
        if self.fixes.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.fixes.len();
        self.adjust_scroll_forward();
    }

    /// Move selection backward, wrapping around
    fn select_previous(&mut self) {
        if self.fixes.is_empty() {
            return;
        }
        self.selected = self.selected.checked_sub(1).unwrap_or(self.fixes.len() - 1);
        self.adjust_scroll_backward();
    }

    /// Adjust scroll position when moving forward
    fn adjust_scroll_forward(&mut self) {
        let visible_items = self.max_height as usize;
        if self.selected >= self.skip_values + visible_items {
            self.skip_values = self.selected.saturating_sub(visible_items - 1);
        } else if self.selected < self.skip_values {
            self.skip_values = self.selected;
        }
    }

    /// Adjust scroll position when moving backward
    fn adjust_scroll_backward(&mut self) {
        if self.selected < self.skip_values {
            self.skip_values = self.selected;
        }
    }
}

/// Extract text edits from a code action's workspace edit.
fn extract_text_edits(action: &CodeAction) -> Option<Vec<TextEdit>> {
    action
        .edit
        .as_ref()?
        .changes
        .as_ref()?
        .values()
        .next()
        .cloned()
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
            MenuEvent::Deactivate => self.active = false,
            MenuEvent::NextElement => self.select_next(),
            MenuEvent::PreviousElement => self.select_previous(),
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
        // Calculate menu position: prompt_width + anchor_col
        // cursor_col = prompt_width + text_before_cursor_width (mod terminal width)
        // So: prompt_width = cursor_col - text_before_cursor_width
        let line_buffer = editor.line_buffer();
        let cursor_visual_width = line_buffer.get_buffer()[..line_buffer
            .insertion_point()
            .min(line_buffer.get_buffer().len())]
            .width() as u16;

        self.working_details.space_left = self
            .working_details
            .cursor_col
            .saturating_sub(cursor_visual_width)
            .saturating_add(self.anchor_col)
            .saturating_sub(LEFT_PADDING);
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        let Some(fix) = self.get_selected_fix() else {
            return;
        };

        // Sort edits by start position descending to apply from end to start
        let mut edits = fix.edits.clone();
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));

        let mut line_buffer = editor.line_buffer().clone();

        // Apply all edits using fold
        let new_buffer = edits.iter().fold(
            line_buffer.get_buffer().to_string(),
            |mut buf, (span, new_text)| {
                let start = span.start.min(buf.len());
                let end = span.end.min(buf.len());
                buf.replace_range(start..end, new_text);
                buf
            },
        );

        // Place cursor at end of first edit
        let cursor_pos = fix
            .edits
            .first()
            .map(|(span, new_text)| span.start + new_text.len())
            .unwrap_or_else(|| line_buffer.insertion_point());

        line_buffer.set_buffer(new_buffer);
        line_buffer.set_insertion_point(cursor_pos.min(line_buffer.get_buffer().len()));
        editor.set_line_buffer(line_buffer, UndoBehavior::CreateUndoPoint);
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

        let visible_count = (available_lines.min(self.max_height)) as usize;
        let left_padding = " ".repeat(self.working_details.space_left as usize);

        self.fixes
            .iter()
            .enumerate()
            .skip(self.skip_values)
            .take(visible_count)
            .map(|(idx, fix)| {
                format!(
                    "{left_padding}{}",
                    self.format_fix_line(fix, idx, use_ansi_coloring)
                )
            })
            .join("\r\n")
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.working_details.cursor_col = pos.0;
    }
}
