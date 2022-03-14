use {
    crossterm::style::Color,
    serde::{Deserialize, Serialize},
    std::{
        borrow::Cow,
        fmt::{Display, Formatter},
    },
    strum_macros::EnumIter,
};

/// The default color for the prompt
pub static DEFAULT_PROMPT_COLOR: Color = Color::Blue;

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
    pub fn new(status: PromptHistorySearchStatus, search_term: String) -> Self {
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
#[derive(Serialize, Deserialize, Clone, Debug, EnumIter)]
pub enum PromptViMode {
    /// The default mode
    Normal,

    /// Insertion mode
    Insert,
}

impl Default for PromptViMode {
    fn default() -> Self {
        PromptViMode::Normal
    }
}

impl Display for PromptEditMode {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            PromptEditMode::Default => write!(f, "Default"),
            PromptEditMode::Emacs => write!(f, "Emacs"),
            PromptEditMode::Vi(_) => write!(f, "Vi_Normal\nVi_Insert"),
            PromptEditMode::Custom(s) => write!(f, "Custom_{}", s),
        }
    }
}
/// API to provide a custom prompt.
///
/// Implementors have to provide [`str`]-based content which will be
/// displayed before the `LineBuffer` is drawn.
pub trait Prompt: Send {
    /// Provide content off the right full prompt
    fn render_prompt_left(&self) -> Cow<str>;
    /// Provide content off the left full prompt
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
    /// Get back the prompt color
    fn get_prompt_color(&self) -> Color {
        DEFAULT_PROMPT_COLOR
    }
}
