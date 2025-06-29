use {
    crossterm::style::Color,
    serde::{Deserialize, Serialize},
    std::{
        borrow::Cow,
        fmt::{Display, Formatter},
    },
    strum_macros::EnumIter,
};

/// The default color for the prompt, indicator, and right prompt
pub static DEFAULT_PROMPT_COLOR: Color = Color::Green;
pub static DEFAULT_PROMPT_MULTILINE_COLOR: nu_ansi_term::Color = nu_ansi_term::Color::LightBlue;
pub static DEFAULT_INDICATOR_COLOR: Color = Color::Cyan;
pub static DEFAULT_PROMPT_RIGHT_COLOR: Color = Color::AnsiValue(5);

/// The current success/failure of the history search
pub enum PromptHistorySearchStatus {
    /// Success for the search
    Passing,

    /// Failure to find the search
    Failing,
}

/// A representation of the history search
pub struct PromptHistorySearch {
    /// The status of the search
    pub status: PromptHistorySearchStatus,

    /// The search term used during the search
    pub term: String,
}

impl PromptHistorySearch {
    /// A constructor to create a history search
    pub const fn new(status: PromptHistorySearchStatus, search_term: String) -> Self {
        PromptHistorySearch {
            status,
            term: search_term,
        }
    }
}

/// Modes that the prompt can be in
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter)]
pub enum PromptEditMode {
    /// The default mode
    Default,

    /// Emacs normal mode
    Emacs,

    /// A vi-specific mode
    Vi(PromptViMode),

    /// A custom mode
    Custom(String),
}

/// The vi-specific modes that the prompt can be in
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter, Default)]
pub enum PromptViMode {
    /// The default mode
    #[default]
    Normal,

    /// Insertion mode
    Insert,
}

impl Display for PromptEditMode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            PromptEditMode::Default => write!(f, "Default"),
            PromptEditMode::Emacs => write!(f, "Emacs"),
            PromptEditMode::Vi(_) => write!(f, "Vi_Normal\nVi_Insert"),
            PromptEditMode::Custom(s) => write!(f, "Custom_{s}"),
        }
    }
}
/// API to provide a custom prompt.
///
/// Implementors have to provide [`str`]-based content which will be
/// displayed before the `LineBuffer` is drawn.
pub trait Prompt: Send {
    /// Provide content of the left full prompt
    fn render_prompt_left(&self) -> Cow<str>;
    /// Provide content of the right full prompt
    fn render_prompt_right(&self) -> Cow<str>;
    /// Render the prompt indicator (Last part of the prompt that changes based on the editor mode)
    fn render_prompt_indicator(&self, prompt_mode: PromptEditMode) -> Cow<str>;
    /// Indicator to show before explicit new lines
    fn render_prompt_multiline_indicator(&self) -> Cow<str>;
    /// Render the prompt indicator for `Ctrl-R` history search
    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str>;
    /// Get the default prompt color
    fn get_prompt_color(&self) -> Color {
        DEFAULT_PROMPT_COLOR
    }
    /// Get the default multiline prompt color
    fn get_prompt_multiline_color(&self) -> nu_ansi_term::Color {
        DEFAULT_PROMPT_MULTILINE_COLOR
    }
    /// Get the default indicator color
    fn get_indicator_color(&self) -> Color {
        DEFAULT_INDICATOR_COLOR
    }
    /// Get the default right prompt color
    fn get_prompt_right_color(&self) -> Color {
        DEFAULT_PROMPT_RIGHT_COLOR
    }

    /// Whether to render right prompt on the last line
    fn right_prompt_on_last_line(&self) -> bool {
        false
    }
}
