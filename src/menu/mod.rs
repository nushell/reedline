mod completion_menu;
mod history_menu;
pub mod menu_functions;

use crate::{painting::Painter, Completer, History, LineBuffer, Suggestion};
pub use completion_menu::CompletionMenu;
pub use history_menu::HistoryMenu;
use nu_ansi_term::{Color, Style};

/// Struct to store the menu style
pub struct MenuTextStyle {
    /// Text style for selected text in a menu
    pub selected_text_style: Style,
    /// Text style for not selected text in the menu
    pub text_style: Style,
    /// Text style for the item description
    pub description_style: Style,
}

impl Default for MenuTextStyle {
    fn default() -> Self {
        Self {
            selected_text_style: Color::Green.bold().reverse(),
            text_style: Color::DarkGray.normal(),
            description_style: Color::Yellow.normal(),
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
    /// Menu name
    fn name(&self) -> &str;

    /// Menu indicator
    fn indicator(&self) -> &str;

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
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    ) -> bool;

    /// Updates the values presented in the menu
    /// This function needs to be defined in the trait because when the menu is
    /// activated or the `quick_completion` option is true, the len of the values
    /// is calculated to know if there is only one value so it can be selected
    /// immediately
    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    );

    /// The working details of a menu are values that could change based on
    /// the menu conditions before it being printed, such as the number or size
    /// of columns, etc.
    /// In this function should be defined how the menu event is treated since
    /// it is called just before painting the menu
    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    );

    /// Indicates how to replace in the line buffer the selected value from the menu
    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer);

    /// Calculates the real required lines for the menu considering how many lines
    /// wrap the terminal or if entries have multiple lines
    fn menu_required_lines(&self, terminal_columns: u16) -> u16;

    /// Creates the menu representation as a string which will be painted by the painter
    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String;

    /// Minimum rows that should be displayed by the menu
    fn min_rows(&self) -> u16;

    /// Gets cached values from menu that will be displayed
    fn get_values(&self) -> &[Suggestion];
}

pub(crate) enum ReedlineMenu {
    /// Menu that uses Reedline's completer to update its values
    EngineCompleter(Box<dyn Menu>),
    /// Menu that has its own Completer
    WithCompleter {
        menu: Box<dyn Menu>,
        completer: Box<dyn Completer>,
    },
}

impl ReedlineMenu {
    fn as_ref(&self) -> &dyn Menu {
        match self {
            Self::EngineCompleter(menu) => menu.as_ref(),
            Self::WithCompleter { menu, .. } => menu.as_ref(),
        }
    }

    fn as_mut(&mut self) -> &mut dyn Menu {
        match self {
            Self::EngineCompleter(menu) => menu.as_mut(),
            Self::WithCompleter { menu, .. } => menu.as_mut(),
        }
    }
}

impl Menu for ReedlineMenu {
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
        self.as_mut().menu_event(event)
    }

    fn can_quick_complete(&self) -> bool {
        self.as_ref().can_quick_complete()
    }

    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    ) -> bool {
        match self {
            Self::EngineCompleter(menu) => {
                menu.can_partially_complete(values_updated, line_buffer, history, completer)
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => menu.can_partially_complete(
                values_updated,
                line_buffer,
                history,
                own_completer.as_ref(),
            ),
        }
    }

    fn update_values(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
    ) {
        match self {
            Self::EngineCompleter(menu) => menu.update_values(line_buffer, history, completer),
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => menu.update_values(line_buffer, history, own_completer.as_ref()),
        }
    }

    fn update_working_details(
        &mut self,
        line_buffer: &mut LineBuffer,
        history: &dyn History,
        completer: &dyn Completer,
        painter: &Painter,
    ) {
        match self {
            Self::EngineCompleter(menu) => {
                menu.update_working_details(line_buffer, history, completer, painter)
            }
            Self::WithCompleter {
                menu,
                completer: own_completer,
            } => menu.update_working_details(line_buffer, history, own_completer.as_ref(), painter),
        }
    }

    fn replace_in_buffer(&self, line_buffer: &mut LineBuffer) {
        self.as_ref().replace_in_buffer(line_buffer)
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
}
