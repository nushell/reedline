mod columnar_menu;
mod description_menu;
mod ide_menu;
mod list_menu;
pub mod menu_functions;

use crate::core_editor::Editor;
use crate::History;
use crate::{completion::history::HistoryCompleter, painting::Painter, Completer, Suggestion};
pub use columnar_menu::ColumnarMenu;
pub use columnar_menu::TraversalDirection;
pub use description_menu::DescriptionMenu;
pub use ide_menu::DescriptionMode;
pub use ide_menu::IdeMenu;
pub use list_menu::DescriptionPosition;
pub use list_menu::ListMenu;
use nu_ansi_term::{Color, Style};

/// Struct to store the menu style
pub struct MenuTextStyle {
    /// Text style for selected text in a menu
    pub selected_text_style: Style,
    /// Text style for not selected text in the menu
    pub text_style: Style,
    /// Text style for the item description
    pub description_style: Style,
    /// Text style of the parts of the suggestions that match the
    /// typed text when the suggestion is selected
    pub selected_match_style: Style,
    /// Text style of the parts of the suggestions that match the
    /// typed text
    pub match_style: Style,
}

impl Default for MenuTextStyle {
    fn default() -> Self {
        Self {
            selected_text_style: Color::Green.bold().reverse(),
            text_style: Color::DarkGray.normal(),
            description_style: Color::Yellow.normal(),
            selected_match_style: Color::Green.bold().reverse().underline(),
            match_style: Style::default().underline(),
        }
    }
}

/// Defines all possible events that could happen with a menu.
#[derive(Clone)]
pub enum MenuEvent {
    /// Activation event for the menu. When the bool is true it means that the values
    /// have already being updated. This is true when the option `quick_completions` is true
    Activate(bool),
    /// Deactivation event
    Deactivate,
    /// Line buffer edit event. When the bool is true it means that the values
    /// have already being updated. This is true when the option `quick_completions` is true
    Edit(bool),
    /// Selecting next element in the menu
    NextElement,
    /// Selecting previous element in the menu
    PreviousElement,
    /// Moving up in the menu
    MoveUp,
    /// Moving down in the menu
    MoveDown,
    /// Moving left in the menu
    MoveLeft,
    /// Moving right in the menu
    MoveRight,
    /// Move to next page
    NextPage,
    /// Move to previous page
    PreviousPage,
}

/// Trait that defines how a menu will be printed by the painter
pub trait Menu: Send {
    /// Get MenuSettings
    fn settings(&self) -> &MenuSettings {
        // We panic here, so this function has base implementation
        // so existing menus will not break.
        // if a breaking change is ok, this can be removed
        panic!(
            "`settings` requires a manual implementation per menu. It has a base implementation to not break existing menus"
        )
    }

    /// Menu name
    fn name(&self) -> &str {
        &self.settings().name
    }

    /// Menu indicator
    fn indicator(&self) -> &str {
        &self.settings().marker
    }

    /// Checks if the menu is active
    fn is_active(&self) -> bool;

    /// Selects what type of event happened with the menu
    fn menu_event(&mut self, event: MenuEvent);

    /// A menu may not be allowed to quick complete because it needs to stay
    /// active even with one element
    fn can_quick_complete(&self) -> bool;

    /// The completion menu can try to find the common string and replace it
    /// in the given line buffer
    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> bool;

    /// Updates the values presented in the menu
    /// This function needs to be defined in the trait because when the menu is
    /// activated or the `quick_completion` option is true, the len of the values
    /// is calculated to know if there is only one value so it can be selected
    /// immediately
    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer);

    /// Resets the menu's selection back to its initial position.
    fn reset_position(&mut self);

    /// Handles a menu event that reloads the menu's contents
    fn reload(&mut self, updated: bool, editor: &mut Editor, completer: &mut dyn Completer) {
        self.reset_position();
        if !updated {
            self.update_values(editor, completer);
        }
    }

    /// The working details of a menu are values that could change based on
    /// the menu conditions before it being printed, such as the number or size
    /// of columns, etc.
    /// In this function should be defined how the menu event is treated since
    /// it is called just before painting the menu
    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    );

    /// Indicates how to replace in the line buffer the selected value from the menu
    fn replace_in_buffer(&self, editor: &mut Editor);

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal or if entries have multiple lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16;

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String;

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16;

    /// Gets cached values from menu that will be displayed
    fn get_values(&self) -> &[Suggestion];
    /// Sets the position of the cursor (currently only required by the IDE menu)
    fn set_cursor_pos(&mut self, _pos: (u16, u16)) {
        // empty implementation to make it optional
    }
}

/// Struct to store configuration for a menu.
pub struct MenuSettings {
    /// Menu name
    name: String,
    /// Menu coloring
    color: MenuTextStyle,
    /// Menu marker when active
    marker: String,
    /// Calls the completer using only the line buffer difference
    /// after the menu was activated. Ignored if `input_mode` is set.
    only_buffer_difference: bool,
    /// Optional override for completer input handling.
    /// If `Some`, takes precedence over `only_buffer_difference`.
    input_mode: Option<InputMode>,
    /// Optional override for the buffer range replaced on selection.
    /// If `None`, the menu uses `Suggestion::span` as-is.
    output_mode: Option<OutputMode>,
}

impl Default for MenuSettings {
    fn default() -> Self {
        Self {
            name: "menu".to_string(),
            color: MenuTextStyle::default(),
            marker: "| ".to_string(),
            only_buffer_difference: false,
            input_mode: None,
            output_mode: None,
        }
    }
}

impl MenuSettings {
    /// MenuSettings builder with name
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// MenuSettings builder with color
    #[must_use]
    pub fn with_color(mut self, color: MenuTextStyle) -> Self {
        self.color = color;
        self
    }

    /// MenuSettings builder with marker
    #[must_use]
    pub fn with_marker(mut self, marker: &str) -> Self {
        self.marker = marker.to_string();
        self
    }

    /// MenuSettings builder with only_buffer_difference.
    /// Consider `with_input_mode` for finer control; the bool is ignored when
    /// `input_mode` is set.
    #[must_use]
    pub fn with_only_buffer_difference(mut self, only_buffer_difference: bool) -> Self {
        self.only_buffer_difference = only_buffer_difference;
        self
    }

    /// Set the input mode. If set, this overrides `only_buffer_difference`.
    #[must_use]
    pub fn with_input_mode(mut self, mode: InputMode) -> Self {
        self.input_mode = Some(mode);
        self
    }

    /// Set the output mode. If unset, the menu uses `Suggestion::span` as-is.
    #[must_use]
    pub fn with_output_mode(mut self, mode: OutputMode) -> Self {
        self.output_mode = Some(mode);
        self
    }

    /// Resolves input_mode and only_buffer_difference into concrete InputMode.
    /// `input_mode` wins if set; otherwise falls back to the bool.
    pub fn effective_input_mode(&self) -> InputMode {
        self.input_mode.unwrap_or(if self.only_buffer_difference {
            InputMode::Diff
        } else {
            InputMode::CursorPrefix
        })
    }
}

/// Controls what the menu hands to its completer.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Completer receives only the text typed after menu activation.
    /// Equivalent to `only_buffer_difference: true`.
    Diff,
    /// Completer receives the buffer up to the cursor (`buffer[..cursor]`).
    /// Equivalent to `only_buffer_difference: false`.
    CursorPrefix,
    /// Completer receives the entire buffer including text after the cursor.
    /// No bool equivalent.
    FullBuffer,
}

/// Controls what range of the buffer the menu replaces when a suggestion is selected.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Replace the range specified by `Suggestion::span`.
    /// Equivalent to leaving `output_mode` unset.
    SuggestedSpan,
    /// Replace the entire buffer (`0..buffer.len()`), ignoring `Suggestion::span`.
    FullBuffer,
    /// Keep `Suggestion::span.start`, force `end = buffer.len()`.
    ExtendToEnd,
}

/// Common builder for all menus
pub trait MenuBuilder: Menu + Sized {
    /// Get mutable MenuSettings
    /// required for the builder functions
    fn settings_mut(&mut self) -> &mut MenuSettings;

    /// Menu builder with new name
    #[must_use]
    fn with_name(mut self, name: &str) -> Self {
        self.settings_mut().name = name.to_string();
        self
    }

    /// Menu builder with new value for text style
    #[must_use]
    fn with_text_style(mut self, color: Style) -> Self {
        self.settings_mut().color.text_style = color;
        self
    }

    /// Menu builder with new value for selected text style
    #[must_use]
    fn with_selected_text_style(mut self, color: Style) -> Self {
        self.settings_mut().color.selected_text_style = color;
        self
    }

    /// Menu builder with new value for description style
    #[must_use]
    fn with_description_text_style(mut self, color: Style) -> Self {
        self.settings_mut().color.description_style = color;
        self
    }

    /// Menu builder with new value for match style
    /// This is the style of the part of the input text, the suggestions
    /// are based on
    #[must_use]
    fn with_match_text_style(mut self, color: Style) -> Self {
        self.settings_mut().color.match_style = color;
        self
    }

    /// Menu builder with new value for selected match style
    /// This is the style of the part of the input text, the suggestions
    /// are based on
    #[must_use]
    fn with_selected_match_text_style(mut self, color: Style) -> Self {
        self.settings_mut().color.selected_match_style = color;
        self
    }

    /// Menu builder with new value for marker
    #[must_use]
    fn with_marker(mut self, marker: &str) -> Self {
        self.settings_mut().marker = marker.to_string();
        self
    }

    /// Menu builder with new value for only_buffer_difference.
    /// Ignored when `input_mode` is set; consider `with_input_mode` for finer control.
    #[must_use]
    fn with_only_buffer_difference(mut self, only_buffer_difference: bool) -> Self {
        self.settings_mut().only_buffer_difference = only_buffer_difference;
        self
    }

    /// Menu builder with new value for input_mode. Overrides `only_buffer_difference` when set.
    #[must_use]
    fn with_input_mode(mut self, mode: InputMode) -> Self {
        self.settings_mut().input_mode = Some(mode);
        self
    }

    /// Menu builder with new value for output_mode. Defaults to `OutputMode::SuggestedSpan` when unset.
    #[must_use]
    fn with_output_mode(mut self, mode: OutputMode) -> Self {
        self.settings_mut().output_mode = Some(mode);
        self
    }
}

/// Allowed menus in Reedline
pub enum ReedlineMenu {
    /// Menu that uses Reedline's completer to update its values
    EngineCompleter(Box<dyn Menu>),
    /// Menu that uses the history as its completer
    HistoryMenu(Box<dyn Menu>),
    /// Menu that has its own Completer
    WithCompleter {
        /// Base menu
        menu: Box<dyn Menu>,
        /// External completer defined outside Reedline
        completer: Box<dyn Completer + Send>,
    },
}

impl ReedlineMenu {
    fn as_ref(&self) -> &dyn Menu {
        match self {
            Self::EngineCompleter(menu)
            | Self::HistoryMenu(menu)
            | Self::WithCompleter { menu, .. } => menu.as_ref(),
        }
    }

    fn as_mut(&mut self) -> &mut dyn Menu {
        match self {
            Self::EngineCompleter(menu)
            | Self::HistoryMenu(menu)
            | Self::WithCompleter { menu, .. } => menu.as_mut(),
        }
    }

    pub(crate) fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        history: &dyn History,
    ) -> bool {
        match self {
            Self::EngineCompleter(menu) => {
                menu.can_partially_complete(values_updated, editor, completer)
            }
            Self::HistoryMenu(menu) => {
                let mut history_completer = HistoryCompleter::new(history);
                menu.can_partially_complete(values_updated, editor, &mut history_completer)
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => menu.can_partially_complete(values_updated, editor, own_completer.as_mut()),
        }
    }

    pub(crate) fn update_values(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        history: &dyn History,
    ) {
        match self {
            Self::EngineCompleter(menu) => menu.update_values(editor, completer),
            Self::HistoryMenu(menu) => {
                let mut history_completer = HistoryCompleter::new(history);
                menu.update_values(editor, &mut history_completer);
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => {
                menu.update_values(editor, own_completer.as_mut());
            }
        }
    }

    pub(crate) fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        history: &dyn History,
        painter: &Painter,
    ) {
        match self {
            Self::EngineCompleter(menu) => {
                menu.update_working_details(editor, completer, painter);
            }
            Self::HistoryMenu(menu) => {
                let mut history_completer = HistoryCompleter::new(history);
                menu.update_working_details(editor, &mut history_completer, painter);
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => {
                menu.update_working_details(editor, own_completer.as_mut(), painter);
            }
        }
    }
}

impl Menu for ReedlineMenu {
    fn settings(&self) -> &MenuSettings {
        self.as_ref().settings()
    }

    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn indicator(&self) -> &str {
        self.as_ref().indicator()
    }

    fn is_active(&self) -> bool {
        self.as_ref().is_active()
    }

    fn menu_event(&mut self, event: MenuEvent) {
        self.as_mut().menu_event(event);
    }

    fn can_quick_complete(&self) -> bool {
        self.as_ref().can_quick_complete()
    }

    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> bool {
        match self {
            Self::EngineCompleter(menu) | Self::HistoryMenu(menu) => {
                menu.can_partially_complete(values_updated, editor, completer)
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => menu.can_partially_complete(values_updated, editor, own_completer.as_mut()),
        }
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        match self {
            Self::EngineCompleter(menu) | Self::HistoryMenu(menu) => {
                menu.update_values(editor, completer);
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => {
                menu.update_values(editor, own_completer.as_mut());
            }
        }
    }

    fn reset_position(&mut self) {
        self.as_mut().reset_position();
    }

    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        match self {
            Self::EngineCompleter(menu) | Self::HistoryMenu(menu) => {
                menu.update_working_details(editor, completer, painter);
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => {
                menu.update_working_details(editor, own_completer.as_mut(), painter);
            }
        }
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        self.as_ref().replace_in_buffer(editor);
    }

    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.as_ref().menu_required_lines(terminal_columns)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        self.as_ref()
            .menu_string(available_lines, use_ansi_coloring)
    }

    fn min_rows(&self) -> u16 {
        self.as_ref().min_rows()
    }

    fn get_values(&self) -> &[Suggestion] {
        self.as_ref().get_values()
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.as_mut().set_cursor_pos(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::bool_only_false(false, None, InputMode::CursorPrefix)]
    #[case::bool_only_true(true, None, InputMode::Diff)]
    #[case::enum_overrides_false_bool(false, Some(InputMode::Diff), InputMode::Diff)]
    #[case::enum_overrides_true_bool(true, Some(InputMode::CursorPrefix), InputMode::CursorPrefix)]
    #[case::full_buffer(true, Some(InputMode::FullBuffer), InputMode::FullBuffer)]
    fn test_effective_input_mode(
        #[case] only_buffer_difference: bool,
        #[case] input_mode: Option<InputMode>,
        #[case] expected: InputMode,
    ) {
        let mut settings =
            MenuSettings::default().with_only_buffer_difference(only_buffer_difference);
        if let Some(mode) = input_mode {
            settings = settings.with_input_mode(mode);
        }
        assert_eq!(settings.effective_input_mode(), expected);
    }
}
